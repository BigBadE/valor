//! Viewport culling and visibility checks.

use rewrite_core::{Axis, Database, Keyword, NodeId};

use super::helpers;

/// Check if a node should be rendered.
pub fn should_render(
    db: &Database,
    node: NodeId,
    viewport_width: f32,
    viewport_height: f32,
) -> bool {
    // Check visibility
    if !is_visible(db, node) {
        return false;
    }

    // Check if in viewport (frustum culling)
    if !is_in_viewport(db, node, viewport_width, viewport_height) {
        return false;
    }

    // Check if size is non-zero
    let width = helpers::get_size(db, node, Axis::Horizontal);
    let height = helpers::get_size(db, node, Axis::Vertical);
    if width <= 0.0 || height <= 0.0 {
        return false;
    }

    true
}

/// Check if node is visible (visibility property).
pub fn is_visible(db: &Database, node: NodeId) -> bool {
    let visibility = helpers::get_visibility(db, node);
    visibility != Keyword::Hidden && visibility != Keyword::Collapse
}

/// Check if node intersects viewport (frustum culling).
pub fn is_in_viewport(
    db: &Database,
    node: NodeId,
    viewport_width: f32,
    viewport_height: f32,
) -> bool {
    use rewrite_core::Edge;

    let x = helpers::get_offset(db, node, Edge::Left);
    let y = helpers::get_offset(db, node, Edge::Top);
    let width = helpers::get_size(db, node, Axis::Horizontal);
    let height = helpers::get_size(db, node, Axis::Vertical);

    helpers::intersects_viewport(x, y, width, height, viewport_width, viewport_height)
}

/// Check if node establishes a containing block.
pub fn establishes_containing_block(db: &Database, node: NodeId) -> bool {
    let position = helpers::get_position(db, node);
    matches!(
        position,
        Keyword::Relative | Keyword::Absolute | Keyword::PositionFixed | Keyword::Sticky
    )
}

/// Check if node has fixed positioning (always in viewport).
pub fn is_fixed(db: &Database, node: NodeId) -> bool {
    let position = helpers::get_position(db, node);
    position == Keyword::PositionFixed
}
