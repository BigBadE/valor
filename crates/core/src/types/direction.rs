//! Directional types used across properties and values.

/// Physical box edge directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Edge {
    Top = 0,
    Right = 1,
    Bottom = 2,
    Left = 3,
}

/// Physical corner positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Corner {
    TopLeft = 0,
    TopRight = 1,
    BottomRight = 2,
    BottomLeft = 3,
}

/// Axis directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Axis {
    Horizontal = 0,
    Vertical = 1,
}

/// 3D axis for transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Axis3D {
    X = 0,
    Y = 1,
    Z = 2,
}

/// Boundary type (min/max constraints).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Boundary {
    Min = 0,
    Max = 1,
}

/// Positional edge (start/end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Position {
    Start = 0,
    End = 1,
}
