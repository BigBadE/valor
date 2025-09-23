/// A rectangle in device-independent pixels (border-box space).
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    /// X coordinate of the border-box origin.
    pub x: i32,
    /// Y coordinate of the border-box origin.
    pub y: i32,
    /// Border-box width.
    pub width: i32,
    /// Border-box height.
    pub height: i32,
}
