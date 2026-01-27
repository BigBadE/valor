//! Visual effects rendering (shadows, filters, transforms).

use rewrite_core::{Axis, Database, Keyword, NodeId};

use crate::DisplayList;

use super::helpers;

pub fn push_transforms(
    _db: &Database,
    _node: NodeId,
    _display_list: &mut Vec<DisplayList>,
) -> bool {
    // TODO: Query transform property
    // TODO: Parse transform functions and push transform commands
    // Return true if a transform was pushed
    false
}

pub fn pop_transforms(pushed: bool, display_list: &mut Vec<DisplayList>) {
    if pushed {
        display_list.push(DisplayList::PopTransform);
    }
}

pub fn push_opacity(_db: &Database, _node: NodeId, _display_list: &mut Vec<DisplayList>) -> bool {
    // TODO: Query opacity property
    // TODO: If opacity < 1.0, push opacity layer
    // Return true if opacity was pushed
    false
}

pub fn pop_opacity(pushed: bool, display_list: &mut Vec<DisplayList>) {
    if pushed {
        display_list.push(DisplayList::PopOpacity);
    }
}

pub fn push_clip(
    db: &Database,
    node: NodeId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    display_list: &mut Vec<DisplayList>,
) -> bool {
    let overflow_x = helpers::get_overflow(db, node, Axis::Horizontal);
    let overflow_y = helpers::get_overflow(db, node, Axis::Vertical);

    // If overflow is not visible, clip
    if overflow_x != Keyword::Visible || overflow_y != Keyword::Visible {
        display_list.push(DisplayList::PushClip {
            x,
            y,
            width,
            height,
        });
        true
    } else {
        false
    }
}

pub fn pop_clip(pushed: bool, display_list: &mut Vec<DisplayList>) {
    if pushed {
        display_list.push(DisplayList::PopClip);
    }
}

pub fn push_stacking_context(
    _db: &Database,
    _node: NodeId,
    _display_list: &mut Vec<DisplayList>,
) -> bool {
    // TODO: Check if node establishes a stacking context
    // - z-index on positioned element
    // - opacity < 1
    // - transform
    // - filter
    // - etc.
    false
}

pub fn pop_stacking_context(pushed: bool, display_list: &mut Vec<DisplayList>) {
    if pushed {
        display_list.push(DisplayList::PopStackingContext);
    }
}

pub fn render_box_shadow(
    _db: &Database,
    _node: NodeId,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _display_list: &mut Vec<DisplayList>,
) {
    // TODO: Query box-shadow property
    // TODO: Parse and render shadows
}

pub fn render_filters(_db: &Database, _node: NodeId, _display_list: &mut Vec<DisplayList>) -> bool {
    // TODO: Query filter property
    // TODO: Push filter commands
    false
}

pub fn pop_filters(pushed: bool, display_list: &mut Vec<DisplayList>) {
    if pushed {
        display_list.push(DisplayList::PopFilter);
    }
}
