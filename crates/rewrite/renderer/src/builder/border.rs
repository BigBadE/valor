//! Border and outline rendering.

use rewrite_core::{Color, Database, Edge, Keyword, NodeId};

use crate::DisplayList;
use crate::display_list::{Border, BorderEdge, BorderRadius};

use super::helpers;

pub fn render_border(
    db: &Database,
    node: NodeId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    display_list: &mut Vec<DisplayList>,
) {
    // Get border widths for all edges
    let widths = helpers::get_edges(|edge| helpers::get_border_width(db, node, edge));

    // Check if any border exists
    if widths.iter().all(|&w| w == 0.0) {
        return;
    }

    // TODO: Query actual border-color and border-style properties
    let color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    // Create border edges
    let edges = widths.map(|width| BorderEdge {
        width,
        style: Keyword::Solid,
        color,
    });

    let border = Border::new(edges[0], edges[1], edges[2], edges[3]);

    display_list.push(DisplayList::DrawBorder {
        x,
        y,
        width,
        height,
        border,
    });
}

pub fn render_outline(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Query outline-width, outline-style, outline-color, outline-offset
    // TODO: Render outline if present
}
