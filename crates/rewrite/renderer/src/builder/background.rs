//! Background rendering.

use rewrite_core::{Color, Database, NodeId};

use crate::DisplayList;

use super::helpers;

pub fn render_background(
    _db: &Database,
    _node: NodeId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    display_list: &mut Vec<DisplayList>,
) {
    // TODO: Query actual background-color property from database
    // For now, render a white background for visible elements

    let color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 0, // Transparent by default
    };

    // Only render if visible
    if color.a > 0 {
        display_list.push(DisplayList::FillRect {
            x,
            y,
            width,
            height,
            color,
        });
    }

    // TODO: Render background-image
    // TODO: Render gradients
    // TODO: Handle background-position, background-size, background-repeat
}
