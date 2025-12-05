//! Sub-pixel layout coordinates using fixed-point arithmetic.
//!
//! Chromium and other browsers use sub-pixel precision to avoid cumulative
//! rounding errors in layout. We use 1/64px units stored as i32, matching
//! Chromium's `LayoutUnit` implementation.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Fixed-point coordinate in 1/64px units.
///
/// This matches Chromium's `LayoutUnit` which uses 6 fractional bits (1/64px precision).
/// All layout coordinates should use this type to maintain sub-pixel precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct LayoutUnit(i32);

impl LayoutUnit {
    /// Number of fractional bits (6 bits = 1/64px precision)
    pub const FRACTIONAL_BITS: u32 = 6;

    /// Scale factor (2^6 = 64)
    pub const SCALE: i32 = 1 << Self::FRACTIONAL_BITS;

    /// Create from raw 1/64px units
    #[inline]
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// Create from pixels (f32)
    #[inline]
    pub fn from_px(pixels: f32) -> Self {
        Self((pixels * Self::SCALE as f32).round() as i32)
    }

    /// Create from pixels (i32)
    #[inline]
    pub const fn from_px_i32(pixels: i32) -> Self {
        Self(pixels * Self::SCALE)
    }

    /// Convert to pixels (f32)
    #[inline]
    pub const fn to_px(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }

    /// Convert to pixels, rounding to nearest integer
    #[inline]
    pub const fn to_px_rounded(self) -> i32 {
        (self.0 + Self::SCALE / 2) / Self::SCALE
    }

    /// Get the raw 1/64px value
    #[inline]
    pub const fn to_raw(self) -> i32 {
        self.0
    }

    /// Convert to pixels, rounding down
    #[inline]
    pub const fn to_px_floor(self) -> i32 {
        self.0 / Self::SCALE
    }

    /// Convert to pixels, rounding up
    #[inline]
    pub const fn to_px_ceil(self) -> i32 {
        (self.0 + Self::SCALE - 1) / Self::SCALE
    }

    /// Get raw value in 1/64px units
    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Zero value
    #[inline]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Maximum value
    #[inline]
    pub const fn max_value() -> Self {
        Self(i32::MAX)
    }

    /// Minimum value
    #[inline]
    pub const fn min_value() -> Self {
        Self(i32::MIN)
    }

    /// Absolute value
    #[inline]
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

// Arithmetic operations
impl Add for LayoutUnit {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for LayoutUnit {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for LayoutUnit {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for LayoutUnit {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Neg for LayoutUnit {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i32> for LayoutUnit {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: i32) -> Self {
        Self(self.0 * rhs)
    }
}

impl Mul<f32> for LayoutUnit {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self((self.0 as f32 * rhs).round() as i32)
    }
}

impl Div<i32> for LayoutUnit {
    type Output = Self;

    #[inline]
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / rhs)
    }
}

impl Div<f32> for LayoutUnit {
    type Output = Self;

    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self((self.0 as f32 / rhs).round() as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test basic conversions between pixels and layout units.
    ///
    /// # Panics
    /// Panics if conversions do not match expected values.
    #[test]
    fn test_conversions() {
        assert!((LayoutUnit::from_px(10.0).to_px() - 10.0).abs() < 0.01);
        assert!((LayoutUnit::from_px(8.328_125).to_px() - 8.328_125).abs() < 0.01);
        assert!((LayoutUnit::from_px_i32(5).to_px() - 5.0).abs() < 0.01);
    }

    /// Test arithmetic operations on layout units.
    ///
    /// # Panics
    /// Panics if arithmetic results do not match expected values.
    #[test]
    fn test_arithmetic() {
        let val_a = LayoutUnit::from_px(10.0);
        let val_b = LayoutUnit::from_px(5.0);

        assert!((val_a + val_b).to_px() - 15.0 < 0.01);
        assert!((val_a - val_b).to_px() - 5.0 < 0.01);
        assert!((val_a * 2).to_px() - 20.0 < 0.01);
        assert!((val_a / 2).to_px() - 5.0 < 0.01);
    }

    /// Test sub-pixel precision with 1/64px units.
    ///
    /// # Panics
    /// Panics if sub-pixel precision is not maintained.
    #[test]
    fn test_subpixel_precision() {
        let x = LayoutUnit::from_px(8.328_125);
        assert_eq!(x.raw(), 533); // 8.328125 * 64 = 533
        assert!((x.to_px() - 8.328_125).abs() < 0.01);
    }
}
