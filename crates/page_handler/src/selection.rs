use js::NodeKey;
use layouter::{LayoutNodeKind, LayoutRect};
use std::collections::HashMap;

pub fn selection_rects(
    rects: &HashMap<NodeKey, LayoutRect>,
    snapshot: &[(NodeKey, LayoutNodeKind, Vec<NodeKey>)],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> Vec<LayoutRect> {
    let sel_x = x0.min(x1);
    let sel_y = y0.min(y1);
    let sel_w = (x0.max(x1) - sel_x).max(0);
    let sel_h = (y0.max(y1) - sel_y).max(0);
    let selection = LayoutRect {
        x: sel_x,
        y: sel_y,
        width: sel_w,
        height: sel_h,
    };
    let mut out: Vec<LayoutRect> = Vec::new();
    for (key, kind, _children) in snapshot.iter() {
        if let LayoutNodeKind::InlineText { text } = kind {
            if text.trim().is_empty() {
                continue;
            }
            if let Some(r) = rects.get(key) {
                let ix = r.x.max(selection.x);
                let iy = r.y.max(selection.y);
                let ix1 = (r.x + r.width).min(selection.x + selection.width);
                let iy1 = (r.y + r.height).min(selection.y + selection.height);
                let iw = (ix1 - ix).max(0);
                let ih = (iy1 - iy).max(0);
                if iw > 0 && ih > 0 {
                    out.push(LayoutRect {
                        x: ix,
                        y: iy,
                        width: iw,
                        height: ih,
                    });
                }
            }
        }
    }
    out
}

pub fn caret_at(
    rects: &HashMap<NodeKey, LayoutRect>,
    snapshot: &[(NodeKey, LayoutNodeKind, Vec<NodeKey>)],
    x: i32,
    y: i32,
    hit: Option<NodeKey>,
) -> Option<LayoutRect> {
    if let Some(hit_key) = hit
        && let Some(r) = rects.get(&hit_key)
        && let Some((_k, LayoutNodeKind::InlineText { .. }, _)) =
            snapshot.iter().find(|(k, _, _)| *k == hit_key)
    {
        let cx = x.clamp(r.x, r.x + r.width);
        return Some(LayoutRect {
            x: cx,
            y: r.y,
            width: 1,
            height: r.height,
        });
    }
    // Fallback: scan inline text rects that contain y and are within the same row
    for (key, kind, _children) in snapshot.iter() {
        if let LayoutNodeKind::InlineText { .. } = kind
            && let Some(r) = rects.get(key)
            && y >= r.y
            && y < r.y + r.height
        {
            let cx = x.clamp(r.x, r.x + r.width);
            return Some(LayoutRect {
                x: cx,
                y: r.y,
                width: 1,
                height: r.height,
            });
        }
    }
    None
}
