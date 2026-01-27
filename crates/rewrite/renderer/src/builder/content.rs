//! Content rendering (text, images, replaced elements).

use rewrite_core::{Database, Keyword, NodeId};

use crate::DisplayList;

use super::helpers;

pub fn render_content(
    db: &Database,
    node: NodeId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    display_list: &mut Vec<DisplayList>,
) {
    // Get display type to determine rendering strategy
    let display = helpers::get_display(db, node);

    // TODO: Get node data to determine node type (element, text, etc.)
    // For now, we'll add placeholders for different content types

    render_text(db, node, x, y, width, height, display_list);
    render_replaced_element(db, node, x, y, width, height, display_list);
}

fn render_text(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Check if node is a text node
    // TODO: Shape text using font, font-size, font-weight, etc.
    // TODO: Apply text-decoration
    // TODO: Apply text-shadow
    // TODO: Handle text selection
    // TODO: Emit DrawText commands
}

fn render_replaced_element(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Check node type (img, video, canvas, svg, iframe, etc.)
    // TODO: Emit appropriate display command:
    //   - DrawImage for <img>
    //   - DrawVideo for <video>
    //   - DrawCanvas for <canvas>
    //   - DrawSvg for <svg>
}

pub fn render_text_decoration(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _width: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Query text-decoration-line, text-decoration-style, text-decoration-color
    // TODO: Emit DrawTextDecoration commands
}

pub fn render_text_shadow(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Query text-shadow property
    // TODO: Parse and emit DrawTextShadow commands
}

pub fn render_focus_ring(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Check if element is focused (needs input state)
    // TODO: Query outline properties for focus ring
    // TODO: Emit DrawFocusRing command
}

pub fn render_list_marker(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Check if element has display: list-item
    // TODO: Query list-style-type, list-style-position
    // TODO: Render bullet/marker
}
