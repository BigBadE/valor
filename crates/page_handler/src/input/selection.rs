use crate::utilities::snapshots::LayoutNodeKind;
use crate::utilities::snapshots::{IRect, SnapshotSlice};
use core::hash::BuildHasher;
use css_core::LayoutRect;
use js::NodeKey;
use std::collections::HashMap;

/// Computes layout rectangles for text nodes that intersect with a selection region.
///
/// This function takes a selection rectangle (in screen coordinates) and returns
/// a list of layout rectangles for all inline text nodes that intersect with it.
/// Empty or whitespace-only text nodes are excluded.
///
/// # Arguments
///
/// * `rects` - Map of node keys to their computed layout rectangles
/// * `snapshot` - The current layout tree snapshot
/// * `sel` - Selection rectangle as `(x0, y0, x1, y1)` in screen coordinates
///
/// # Returns
///
/// A vector of `LayoutRect` representing the intersection areas between
/// the selection and visible text nodes.
#[must_use]
pub fn selection_rects<S: BuildHasher>(
    rects: &HashMap<NodeKey, LayoutRect, S>,
    snapshot: SnapshotSlice,
    sel: IRect,
) -> Vec<LayoutRect> {
    let (x0, y0, x1, y1) = sel;
    let sel_x = x0.min(x1) as f32;
    let sel_y = y0.min(y1) as f32;

    let sel_w = (x0.max(x1) - sel_x.round() as i32).max(0i32) as f32;
    let sel_h = (y0.max(y1) - sel_y.round() as i32).max(0i32) as f32;
    let selection = LayoutRect {
        x: sel_x,
        y: sel_y,
        width: sel_w,
        height: sel_h,
    };
    let mut out: Vec<LayoutRect> = Vec::new();
    for (key, kind, _children) in snapshot {
        if let LayoutNodeKind::InlineText { text } = kind {
            if text.trim().is_empty() {
                continue;
            }
            if let Some(rect) = rects.get(key) {
                let intersect_left = rect.x.max(selection.x);
                let intersect_top = rect.y.max(selection.y);
                let intersect_right = (rect.x + rect.width).min(selection.x + selection.width);
                let intersect_bottom = (rect.y + rect.height).min(selection.y + selection.height);
                let intersect_width = (intersect_right - intersect_left).max(0.0);
                let intersect_height = (intersect_bottom - intersect_top).max(0.0);
                if intersect_width > 0.0 && intersect_height > 0.0 {
                    out.push(LayoutRect {
                        x: intersect_left,
                        y: intersect_top,
                        width: intersect_width,
                        height: intersect_height,
                    });
                }
            }
        }
    }
    out
}

/// Computes a caret rectangle at the specified screen coordinates.
///
/// If a `hit` node key is provided and corresponds to an inline text node,
/// the caret is positioned within that node's bounding box. Otherwise, falls
/// back to scanning for any inline text node at the given y-coordinate.
///
/// # Arguments
///
/// * `rects` - Map of node keys to their computed layout rectangles
/// * `snapshot` - The current layout tree snapshot
/// * `x_coord` - X coordinate in screen space
/// * `y_coord` - Y coordinate in screen space
/// * `hit` - Optional node key from a hit test result
///
/// # Returns
///
/// A 1px-wide `LayoutRect` representing the caret position, or `None` if
/// no suitable text node is found.
#[must_use]
pub fn caret_at<S: BuildHasher>(
    rects: &HashMap<NodeKey, LayoutRect, S>,
    snapshot: SnapshotSlice,
    x_coord: i32,
    y_coord: i32,
    hit: Option<NodeKey>,
) -> Option<LayoutRect> {
    if let Some(hit_key) = hit
        && let Some(rect) = rects.get(&hit_key)
        && let Some(((), LayoutNodeKind::InlineText { .. }, ())) = snapshot
            .iter()
            .find(|(key, _, _)| *key == hit_key)
            .map(|(_, kind, _)| ((), kind, ()))
    {
        let caret_x = (x_coord as f32).clamp(rect.x, rect.x + rect.width);
        return Some(LayoutRect {
            x: caret_x,
            y: rect.y,
            width: 1.0,
            height: rect.height,
        });
    }
    // Fallback: scan inline text rects that contain y_coord and are within the same row
    for (key, kind, _children) in snapshot {
        if let LayoutNodeKind::InlineText { .. } = kind
            && let Some(rect) = rects.get(key)
            && y_coord >= rect.y.round() as i32
            && y_coord < (rect.y + rect.height).round() as i32
        {
            let caret_x = (x_coord as f32).clamp(rect.x, rect.x + rect.width);
            return Some(LayoutRect {
                x: caret_x,
                y: rect.y,
                width: 1.0,
                height: rect.height,
            });
        }
    }
    None
}
