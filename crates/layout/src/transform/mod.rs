/// CSS Transform module implementing CSS Transforms Level 1 and 2.
///
/// This module handles:
/// - 2D and 3D transformations
/// - Transform matrix operations
/// - Transform origin
/// - Coordinate space transformations
/// - Transform composition
///
/// Spec: https://www.w3.org/TR/css-transforms-1/
/// Spec: https://www.w3.org/TR/css-transforms-2/
use crate::Subpixels;

/// 2D transformation matrix in column-major order.
///
/// Matrix layout:
/// ```text
/// [ a  c  e ]   [ sx  shy tx ]
/// [ b  d  f ] = [ shx sy  ty ]
/// [ 0  0  1 ]   [ 0   0   1  ]
/// ```
///
/// Where:
/// - (a, d) = scale
/// - (b, c) = skew/rotation
/// - (e, f) = translate
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Transform2D {
    /// Identity transform (no transformation).
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Create a translation transform.
    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }

    /// Create a scale transform.
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Create a rotation transform (angle in radians).
    pub fn rotate(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Create a skewX transform (angle in radians).
    pub fn skew_x(angle: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: angle.tan(),
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Create a skewY transform (angle in radians).
    pub fn skew_y(angle: f32) -> Self {
        Self {
            a: 1.0,
            b: angle.tan(),
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Multiply this transform by another (compose transformations).
    ///
    /// Returns `self * other` (applies `other` then `self`).
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    /// Transform a point.
    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Transform a vector (ignores translation).
    pub fn transform_vector(&self, dx: f32, dy: f32) -> (f32, f32) {
        (self.a * dx + self.c * dy, self.b * dx + self.d * dy)
    }

    /// Compute the inverse transformation.
    ///
    /// Returns None if the matrix is not invertible (determinant is zero).
    pub fn inverse(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-10 {
            return None;
        }

        let inv_det = 1.0 / det;
        Some(Self {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            e: (self.c * self.f - self.d * self.e) * inv_det,
            f: (self.b * self.e - self.a * self.f) * inv_det,
        })
    }

    /// Check if this is the identity transform.
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < 1e-6
            && self.b.abs() < 1e-6
            && self.c.abs() < 1e-6
            && (self.d - 1.0).abs() < 1e-6
            && self.e.abs() < 1e-6
            && self.f.abs() < 1e-6
    }

    /// Decompose into translation, rotation, scale, and skew components.
    ///
    /// This is useful for animation and debugging.
    pub fn decompose(&self) -> DecomposedTransform2D {
        // Extract translation
        let translate_x = self.e;
        let translate_y = self.f;

        // Extract scale and rotation
        let scale_x = (self.a * self.a + self.b * self.b).sqrt();
        let scale_y = (self.c * self.c + self.d * self.d).sqrt();

        // Normalize to get rotation
        let angle = self.b.atan2(self.a);

        // Extract skew
        let skew = if scale_x != 0.0 {
            (self.a * self.c + self.b * self.d) / (scale_x * scale_x)
        } else {
            0.0
        };

        DecomposedTransform2D {
            translate_x,
            translate_y,
            scale_x,
            scale_y,
            rotate: angle,
            skew,
        }
    }
}

/// Decomposed 2D transform components.
#[derive(Debug, Clone, Copy)]
pub struct DecomposedTransform2D {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotate: f32, // radians
    pub skew: f32,
}

/// 3D transformation matrix (4x4 in column-major order).
///
/// Matrix layout:
/// ```text
/// [ m11 m21 m31 m41 ]
/// [ m12 m22 m32 m42 ]
/// [ m13 m23 m33 m43 ]
/// [ m14 m24 m34 m44 ]
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform3D {
    pub m11: f32,
    pub m12: f32,
    pub m13: f32,
    pub m14: f32,
    pub m21: f32,
    pub m22: f32,
    pub m23: f32,
    pub m24: f32,
    pub m31: f32,
    pub m32: f32,
    pub m33: f32,
    pub m34: f32,
    pub m41: f32,
    pub m42: f32,
    pub m43: f32,
    pub m44: f32,
}

impl Transform3D {
    /// Identity transform.
    pub const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m13: 0.0,
        m14: 0.0,
        m21: 0.0,
        m22: 1.0,
        m23: 0.0,
        m24: 0.0,
        m31: 0.0,
        m32: 0.0,
        m33: 1.0,
        m34: 0.0,
        m41: 0.0,
        m42: 0.0,
        m43: 0.0,
        m44: 1.0,
    };

    /// Create from a 2D transform.
    pub fn from_2d(transform: &Transform2D) -> Self {
        Self {
            m11: transform.a,
            m12: transform.b,
            m13: 0.0,
            m14: 0.0,
            m21: transform.c,
            m22: transform.d,
            m23: 0.0,
            m24: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
            m34: 0.0,
            m41: transform.e,
            m42: transform.f,
            m43: 0.0,
            m44: 1.0,
        }
    }

    /// Create a 3D translation.
    pub fn translate_3d(tx: f32, ty: f32, tz: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.m41 = tx;
        m.m42 = ty;
        m.m43 = tz;
        m
    }

    /// Create a 3D scale.
    pub fn scale_3d(sx: f32, sy: f32, sz: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.m11 = sx;
        m.m22 = sy;
        m.m33 = sz;
        m
    }

    /// Create a rotation around the X axis (angle in radians).
    pub fn rotate_x(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        let mut m = Self::IDENTITY;
        m.m22 = cos;
        m.m23 = sin;
        m.m32 = -sin;
        m.m33 = cos;
        m
    }

    /// Create a rotation around the Y axis (angle in radians).
    pub fn rotate_y(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        let mut m = Self::IDENTITY;
        m.m11 = cos;
        m.m13 = -sin;
        m.m31 = sin;
        m.m33 = cos;
        m
    }

    /// Create a rotation around the Z axis (angle in radians).
    pub fn rotate_z(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        let mut m = Self::IDENTITY;
        m.m11 = cos;
        m.m12 = sin;
        m.m21 = -sin;
        m.m22 = cos;
        m
    }

    /// Transform a 3D point.
    pub fn transform_point_3d(&self, x: f32, y: f32, z: f32) -> (f32, f32, f32) {
        let w = self.m14 * x + self.m24 * y + self.m34 * z + self.m44;
        let w_inv = if w != 0.0 { 1.0 / w } else { 1.0 };

        (
            (self.m11 * x + self.m21 * y + self.m31 * z + self.m41) * w_inv,
            (self.m12 * x + self.m22 * y + self.m32 * z + self.m42) * w_inv,
            (self.m13 * x + self.m23 * y + self.m33 * z + self.m43) * w_inv,
        )
    }

    /// Multiply this transform by another.
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            m11: self.m11 * other.m11
                + self.m21 * other.m12
                + self.m31 * other.m13
                + self.m41 * other.m14,
            m12: self.m12 * other.m11
                + self.m22 * other.m12
                + self.m32 * other.m13
                + self.m42 * other.m14,
            m13: self.m13 * other.m11
                + self.m23 * other.m12
                + self.m33 * other.m13
                + self.m43 * other.m14,
            m14: self.m14 * other.m11
                + self.m24 * other.m12
                + self.m34 * other.m13
                + self.m44 * other.m14,

            m21: self.m11 * other.m21
                + self.m21 * other.m22
                + self.m31 * other.m23
                + self.m41 * other.m24,
            m22: self.m12 * other.m21
                + self.m22 * other.m22
                + self.m32 * other.m23
                + self.m42 * other.m24,
            m23: self.m13 * other.m21
                + self.m23 * other.m22
                + self.m33 * other.m23
                + self.m43 * other.m24,
            m24: self.m14 * other.m21
                + self.m24 * other.m22
                + self.m34 * other.m23
                + self.m44 * other.m24,

            m31: self.m11 * other.m31
                + self.m21 * other.m32
                + self.m31 * other.m33
                + self.m41 * other.m34,
            m32: self.m12 * other.m31
                + self.m22 * other.m32
                + self.m32 * other.m33
                + self.m42 * other.m34,
            m33: self.m13 * other.m31
                + self.m23 * other.m32
                + self.m33 * other.m33
                + self.m43 * other.m34,
            m34: self.m14 * other.m31
                + self.m24 * other.m32
                + self.m34 * other.m33
                + self.m44 * other.m34,

            m41: self.m11 * other.m41
                + self.m21 * other.m42
                + self.m31 * other.m43
                + self.m41 * other.m44,
            m42: self.m12 * other.m41
                + self.m22 * other.m42
                + self.m32 * other.m43
                + self.m42 * other.m44,
            m43: self.m13 * other.m41
                + self.m23 * other.m42
                + self.m33 * other.m43
                + self.m43 * other.m44,
            m44: self.m14 * other.m41
                + self.m24 * other.m42
                + self.m34 * other.m43
                + self.m44 * other.m44,
        }
    }
}

/// Transform origin point (for applying transforms around a specific point).
#[derive(Debug, Clone, Copy)]
pub struct TransformOrigin {
    pub x: Subpixels,
    pub y: Subpixels,
    pub z: Subpixels,
}

impl TransformOrigin {
    /// Create transform origin at center (50% 50%).
    pub fn center(width: Subpixels, height: Subpixels) -> Self {
        Self {
            x: width / 2,
            y: height / 2,
            z: 0,
        }
    }

    /// Create transform origin at top-left (0% 0%).
    pub fn top_left() -> Self {
        Self { x: 0, y: 0, z: 0 }
    }

    /// Apply transform around this origin.
    pub fn apply_transform(&self, transform: &Transform2D) -> Transform2D {
        // Translate to origin, apply transform, translate back
        let x = self.x as f32 / 64.0; // Convert from subpixels
        let y = self.y as f32 / 64.0;

        Transform2D::translate(-x, -y)
            .multiply(transform)
            .multiply(&Transform2D::translate(x, y))
    }
}

/// Parse transform functions from CSS value.
///
/// This is a placeholder for actual CSS parsing integration.
pub fn parse_transform(_transform_value: &str) -> Transform2D {
    // TODO: Implement CSS transform parsing
    // For now, return identity
    Transform2D::IDENTITY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_identity() {
        let t = Transform2D::IDENTITY;
        assert!(t.is_identity());
    }

    #[test]
    fn test_transform_translate() {
        let t = Transform2D::translate(10.0, 20.0);
        let (x, y) = t.transform_point(0.0, 0.0);
        assert_eq!(x, 10.0);
        assert_eq!(y, 20.0);
    }

    #[test]
    fn test_transform_scale() {
        let t = Transform2D::scale(2.0, 3.0);
        let (x, y) = t.transform_point(10.0, 10.0);
        assert_eq!(x, 20.0);
        assert_eq!(y, 30.0);
    }

    #[test]
    fn test_transform_compose() {
        let t1 = Transform2D::translate(10.0, 0.0);
        let t2 = Transform2D::scale(2.0, 2.0);
        let composed = t1.multiply(&t2);

        let (x, y) = composed.transform_point(5.0, 5.0);
        assert_eq!(x, 20.0); // (5 * 2) + 10
        assert_eq!(y, 10.0); // 5 * 2
    }

    #[test]
    fn test_transform_inverse() {
        let t = Transform2D::translate(10.0, 20.0);
        let inv = t.inverse().unwrap();

        let (x, y) = t.transform_point(5.0, 5.0);
        let (x2, y2) = inv.transform_point(x, y);

        assert!((x2 - 5.0).abs() < 0.001);
        assert!((y2 - 5.0).abs() < 0.001);
    }
}
