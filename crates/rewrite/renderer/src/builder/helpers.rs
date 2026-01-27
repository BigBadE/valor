//! Generic helper functions for querying layout properties.

use rewrite_core::{
    Axis, Color, CssLayoutProperty, CssValueProperty, Database, Edge, Keyword, NodeId,
};

/// Convert subpixels to pixels.
#[inline]
pub fn subpixels_to_pixels(subpixels: i32) -> f32 {
    subpixels as f32 / 64.0
}

/// Get offset for an edge (position).
#[inline]
pub fn get_offset(db: &Database, node: NodeId, edge: Edge) -> f32 {
    let subpixels = db.get_value_property(node, CssValueProperty::Offset(edge));
    subpixels_to_pixels(subpixels)
}

/// Get size for an axis.
#[inline]
pub fn get_size(db: &Database, node: NodeId, axis: Axis) -> f32 {
    let subpixels = db.get_value_property(node, CssValueProperty::Size(axis));
    subpixels_to_pixels(subpixels)
}

/// Get padding for an edge.
#[inline]
pub fn get_padding(db: &Database, node: NodeId, edge: Edge) -> f32 {
    let subpixels = db.get_value_property(node, CssValueProperty::Padding(edge));
    subpixels_to_pixels(subpixels)
}

/// Get margin for an edge.
#[inline]
pub fn get_margin(db: &Database, node: NodeId, edge: Edge) -> f32 {
    let subpixels = db.get_value_property(node, CssValueProperty::Margin(edge));
    subpixels_to_pixels(subpixels)
}

/// Get border width for an edge.
#[inline]
pub fn get_border_width(db: &Database, node: NodeId, edge: Edge) -> f32 {
    let subpixels = db.get_value_property(node, CssValueProperty::BorderWidth(edge));
    subpixels_to_pixels(subpixels)
}

/// Get layout keyword.
#[inline]
pub fn get_layout_keyword(db: &Database, node: NodeId, prop: CssLayoutProperty) -> Keyword {
    db.get_layout_keyword(node, prop)
}

/// Get overflow for an axis.
#[inline]
pub fn get_overflow(db: &Database, node: NodeId, axis: Axis) -> Keyword {
    db.get_layout_keyword(node, CssLayoutProperty::OverflowAxis(axis))
}

/// Get display type.
#[inline]
pub fn get_display(db: &Database, node: NodeId) -> Keyword {
    db.get_layout_keyword(node, CssLayoutProperty::Display)
}

/// Get position type.
#[inline]
pub fn get_position(db: &Database, node: NodeId) -> Keyword {
    db.get_layout_keyword(node, CssLayoutProperty::Position)
}

/// Get visibility.
#[inline]
pub fn get_visibility(db: &Database, node: NodeId) -> Keyword {
    db.get_layout_keyword(node, CssLayoutProperty::Visibility)
}

/// Check if a rectangle intersects the viewport.
pub fn intersects_viewport(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> bool {
    !(x + width < 0.0 || y + height < 0.0 || x > viewport_width || y > viewport_height)
}

/// Get all four edges as an array.
pub fn get_edges<T>(get_fn: impl Fn(Edge) -> T) -> [T; 4] {
    [
        get_fn(Edge::Top),
        get_fn(Edge::Right),
        get_fn(Edge::Bottom),
        get_fn(Edge::Left),
    ]
}
