//! Transform types (2D and 3D).

/// 2D transformation matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    /// Matrix values [a, b, c, d, e, f] for:
    /// | a  c  e |
    /// | b  d  f |
    /// | 0  0  1 |
    pub matrix: [f32; 6],
}

impl Transform2D {
    /// Identity transform.
    pub const fn identity() -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }

    /// Translation.
    pub const fn translate(x: f32, y: f32) -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0, x, y],
        }
    }

    /// Rotation (angle in radians).
    pub fn rotate(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            matrix: [cos, sin, -sin, cos, 0.0, 0.0],
        }
    }

    /// Scale.
    pub const fn scale(sx: f32, sy: f32) -> Self {
        Self {
            matrix: [sx, 0.0, 0.0, sy, 0.0, 0.0],
        }
    }

    /// Skew (angles in radians).
    pub fn skew(x: f32, y: f32) -> Self {
        Self {
            matrix: [1.0, y.tan(), x.tan(), 1.0, 0.0, 0.0],
        }
    }

    /// Custom matrix.
    pub const fn matrix(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Self {
            matrix: [a, b, c, d, e, f],
        }
    }
}

/// 3D transformation matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform3D {
    /// 4x4 matrix in column-major order.
    pub matrix: [f32; 16],
}

impl Transform3D {
    /// Identity transform.
    pub const fn identity() -> Self {
        #[rustfmt::skip]
        let matrix = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { matrix }
    }

    /// 3D translation.
    pub const fn translate(x: f32, y: f32, z: f32) -> Self {
        #[rustfmt::skip]
        let matrix = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            x,   y,   z,   1.0,
        ];
        Self { matrix }
    }

    /// 3D scale.
    pub const fn scale(sx: f32, sy: f32, sz: f32) -> Self {
        #[rustfmt::skip]
        let matrix = [
            sx,  0.0, 0.0, 0.0,
            0.0, sy,  0.0, 0.0,
            0.0, 0.0, sz,  0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { matrix }
    }

    /// Rotation around X axis (angle in radians).
    pub fn rotate_x(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        #[rustfmt::skip]
        let matrix = [
            1.0, 0.0,  0.0, 0.0,
            0.0, cos,  sin, 0.0,
            0.0, -sin, cos, 0.0,
            0.0, 0.0,  0.0, 1.0,
        ];
        Self { matrix }
    }

    /// Rotation around Y axis (angle in radians).
    pub fn rotate_y(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        #[rustfmt::skip]
        let matrix = [
            cos, 0.0, -sin, 0.0,
            0.0, 1.0, 0.0,  0.0,
            sin, 0.0, cos,  0.0,
            0.0, 0.0, 0.0,  1.0,
        ];
        Self { matrix }
    }

    /// Rotation around Z axis (angle in radians).
    pub fn rotate_z(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        #[rustfmt::skip]
        let matrix = [
            cos,  sin, 0.0, 0.0,
            -sin, cos, 0.0, 0.0,
            0.0,  0.0, 1.0, 0.0,
            0.0,  0.0, 0.0, 1.0,
        ];
        Self { matrix }
    }

    /// Perspective projection.
    pub fn perspective(distance: f32) -> Self {
        #[rustfmt::skip]
        let matrix = [
            1.0, 0.0, 0.0,          0.0,
            0.0, 1.0, 0.0,          0.0,
            0.0, 0.0, 1.0,          -1.0 / distance,
            0.0, 0.0, 0.0,          1.0,
        ];
        Self { matrix }
    }

    /// Custom 4x4 matrix (column-major order).
    pub const fn matrix(m: [f32; 16]) -> Self {
        Self { matrix: m }
    }
}
