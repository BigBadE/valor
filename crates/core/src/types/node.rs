//! Node identifiers and pixel types.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Unique identifier for a DOM node. Index into `DomTree`'s parallel vecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const ROOT: Self = Self(0);
}

/// Fixed-point subpixel value for layout computations.
///
/// Stores value × 64 internally for 1/64 pixel precision (6 fractional bits).
/// All arithmetic operates on the raw fixed-point representation.
/// Uses `i64` to avoid overflow in intermediate calculations (e.g., scaled
/// flex-shrink factors which multiply multiple pixel-scale values).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Subpixel(i64);

impl Subpixel {
    /// The fixed-point scale factor.
    const SCALE: i64 = 64;
    /// Bit shift equivalent of SCALE (log2(64) = 6).
    const SHIFT: u32 = 6;

    /// Zero value.
    pub const ZERO: Self = Self(0);

    /// Create from a whole-pixel integer (e.g., viewport width).
    #[must_use]
    pub const fn from_px(pixels: i32) -> Self {
        Self((pixels as i64) << Self::SHIFT)
    }

    /// Create from a float pixel value (e.g., CSS length resolution).
    #[must_use]
    pub fn from_f32(pixels: f32) -> Self {
        Self((pixels * Self::SCALE as f32).round() as i64)
    }

    /// Create from a raw integer without scaling.
    /// Used for divisors, counts, and indices that are not pixel values.
    #[must_use]
    pub const fn raw(value: i32) -> Self {
        Self(value as i64)
    }

    /// Convert to `f32` pixels (for font-size multiplication, etc.).
    #[must_use]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }

    /// Convert to `f64` pixels (for JSON serialization).
    #[must_use]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / Self::SCALE as f64
    }

    /// Absolute value.
    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl Add for Subpixel {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Subpixel {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Mul for Subpixel {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl Div for Subpixel {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            Self(0)
        } else {
            // Round-half-away-from-zero division to match Chromium's LayoutUnit.
            let quot = self.0 / rhs.0;
            let rem = self.0 % rhs.0;
            // Round if remainder ≥ half the divisor (magnitude-wise).
            if rem.abs() * 2 >= rhs.0.abs() {
                Self(quot + if (self.0 ^ rhs.0) < 0 { -1 } else { 1 })
            } else {
                Self(quot)
            }
        }
    }
}

impl AddAssign for Subpixel {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Subpixel {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Neg for Subpixel {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Sum for Subpixel {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, x| acc + x)
    }
}

impl<'iter> Sum<&'iter Self> for Subpixel {
    fn sum<I: Iterator<Item = &'iter Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, x| acc + *x)
    }
}

impl fmt::Debug for Subpixel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}px", self.to_f32())
    }
}

impl fmt::Display for Subpixel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.to_f64();
        // Format without trailing zeros for clean output
        if val.fract() == 0.0 {
            write!(f, "{}", val as i64)
        } else {
            write!(f, "{val}")
        }
    }
}
