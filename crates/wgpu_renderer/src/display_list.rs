//! Retained display list (DL) primitives and a minimal diffing API.
//!
//! Phase 6 introduces a GPU-friendly retained display list so that we can:
//! - Build paint commands from layout once per frame (or just for dirty regions),
//! - Diff two lists and submit only minimal updates to the renderer, and
//! - Keep a stable, engine-agnostic representation for later compositing.

use crate::renderer::{DrawRect, DrawText};

/// Framebuffer pixel bounds for text (left, top, right, bottom).
pub type TextBoundsPx = (i32, i32, i32, i32);

// Compact aliases to keep tuple-heavy types readable and satisfy clippy's type_complexity.
pub type Scissor = (u32, u32, u32, u32);

/// A single display list item.
/// This MVP focuses on rectangles and text, with lightweight placeholders for
/// clips and opacity that can be wired up later without breaking the API.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    /// Solid color rectangle in device-independent pixels. RGBA with premultiplied alpha not required; alpha in [0,1].
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
    },
    /// Text glyph run in device-independent pixels.
    Text {
        x: f32,
        y: f32,
        text: String,
        color: [f32; 3],
        font_size: f32,
        /// Optional bounds for wrapping/clipping in framebuffer pixels.
        bounds: Option<TextBoundsPx>,
    },
    /// Push a rectangular clip onto the stack (placeholder; enforced as a scissor in RenderState for now).
    BeginClip {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// Pop the most recent clip from the stack.
    EndClip,
    /// Multiply subsequent drawing by an opacity value in [0, 1] (placeholder; batching applies alpha directly to colors).
    Opacity { alpha: f32 },
}

impl Default for DisplayList {
    fn default() -> Self {
        Self::new()
    }
}

/// A retained display list with a monotonically increasing generation counter.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayList {
    /// Linear sequence of display items to be drawn in order.
    pub items: Vec<DisplayItem>,
    /// Generation tag for debugging and quick equality checks across frames.
    pub generation: u64,
}

impl DisplayList {
    /// Create an empty display list.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            generation: 0,
        }
    }

    /// Create a display list from a collection of items.
    pub fn from_items<I: IntoIterator<Item = DisplayItem>>(items: I) -> Self {
        let mut list = Self::new();
        list.items.extend(items);
        list
    }

    /// Bump the generation counter and return the new value.
    pub fn bump_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    /// Append an item to the end of the list.
    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    /// Return a simple diff between two display lists.
    ///
    /// MVP strategy: if the lists are exactly equal, return NoChange; otherwise
    /// request a ReplaceAll with the target items. This keeps the API stable for
    /// future fine-grained diffs without overengineering now.
    pub fn diff(&self, other: &DisplayList) -> DisplayListDiff {
        if self == other {
            DisplayListDiff::NoChange
        } else {
            DisplayListDiff::ReplaceAll(other.items.clone())
        }
    }

    /// Flatten this display list into the current immediate-mode renderer commands.
    /// Returns a pair of (rectangles, text) to feed into RenderState for drawing.
    pub fn flatten_to_immediate(&self) -> (Vec<DrawRect>, Vec<DrawText>) {
        let mut rects: Vec<DrawRect> = Vec::new();
        let mut texts: Vec<DrawText> = Vec::new();
        // Note: clip/opacity are currently ignored by the immediate path.
        for item in &self.items {
            match item {
                DisplayItem::Rect {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => {
                    let rgb = [color[0], color[1], color[2]]; // immediate path drops alpha
                    rects.push(DrawRect {
                        x: *x,
                        y: *y,
                        width: *width,
                        height: *height,
                        color: rgb,
                    })
                }
                DisplayItem::Text {
                    x,
                    y,
                    text,
                    color,
                    font_size,
                    bounds,
                } => texts.push(DrawText {
                    x: *x,
                    y: *y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    bounds: *bounds,
                }),
                DisplayItem::BeginClip { .. }
                | DisplayItem::EndClip
                | DisplayItem::Opacity { .. } => {
                    // Placeholders for future stateful rendering path.
                }
            }
        }
        (rects, texts)
    }
}

/// A coarse-grained diff describing how to transform one display list into another.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayListDiff {
    /// The two lists are identical; no work needed.
    NoChange,
    /// Replace the entire list with the provided items.
    ReplaceAll(Vec<DisplayItem>),
}

/// A simple rectangle primitive used in CPU-side batching tests and for building GPU vertices.
#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
}

/// A batch of drawing work that shares the same scissor rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub scissor: Option<Scissor>,
    pub quads: Vec<Quad>,
}

/// Compute batches from a DisplayList by segmenting on clip stack boundaries.
/// - Returns batches each carrying a list of quads and an optional scissor rect in framebuffer pixels.
/// - Applies Opacity by multiplying per-quad alpha (no offscreen compositing yet).
pub fn batch_display_list(
    list: &DisplayList,
    framebuffer_width: u32,
    framebuffer_height: u32,
) -> Vec<Batch> {
    let mut batches: Vec<Batch> = Vec::new();
    let mut current_quads: Vec<Quad> = Vec::new();
    let mut scissor_stack: Vec<Scissor> = Vec::new();
    let mut current_scissor: Option<Scissor> = None;
    let mut current_opacity: f32 = 1.0;

    let rect_to_scissor = |x: f32, y: f32, w: f32, h: f32| -> Scissor {
        let framebuffer_w = framebuffer_width.max(1);
        let framebuffer_h = framebuffer_height.max(1);
        let mut sx = x.max(0.0).floor() as i32;
        let mut sy = y.max(0.0).floor() as i32;
        let mut sw = w.max(0.0).ceil() as i32;
        let mut sh = h.max(0.0).ceil() as i32;
        if sx < 0 {
            sw += sx;
            sx = 0;
        }
        if sy < 0 {
            sh += sy;
            sy = 0;
        }
        let max_w = framebuffer_w as i32 - sx;
        let max_h = framebuffer_h as i32 - sy;
        let sw = sw.clamp(0, max_w) as u32;
        let sh = sh.clamp(0, max_h) as u32;
        (sx as u32, sy as u32, sw, sh)
    };
    let intersect = |a: Scissor, b: Scissor| -> Scissor {
        let (ax, ay, aw, ah) = a;
        let (bx, by, bw, bh) = b;
        let x0 = ax.max(bx);
        let y0 = ay.max(by);
        let x1 = (ax + aw).min(bx + bw);
        let y1 = (ay + ah).min(by + bh);
        let w = x1.saturating_sub(x0);
        let h = y1.saturating_sub(y0);
        (x0, y0, w, h)
    };

    for item in &list.items {
        match item {
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                color,
            } => {
                let mut rgba = *color;
                rgba[3] *= current_opacity;
                if *width > 0.0 && *height > 0.0 {
                    current_quads.push(Quad {
                        x: *x,
                        y: *y,
                        width: *width,
                        height: *height,
                        color: rgba,
                    });
                }
            }
            DisplayItem::BeginClip {
                x,
                y,
                width,
                height,
            } => {
                // Flush current batch before pushing new scissor
                if !current_quads.is_empty() {
                    batches.push(Batch {
                        scissor: current_scissor,
                        quads: std::mem::take(&mut current_quads),
                    });
                }
                let new_sc = rect_to_scissor(*x, *y, *width, *height);
                let effective = match current_scissor {
                    Some(sc) => intersect(sc, new_sc),
                    None => new_sc,
                };
                scissor_stack.push(new_sc);
                current_scissor = Some(effective);
            }
            DisplayItem::EndClip => {
                // Flush current batch before restoring scissor
                if !current_quads.is_empty() {
                    batches.push(Batch {
                        scissor: current_scissor,
                        quads: std::mem::take(&mut current_quads),
                    });
                }
                let _ = scissor_stack.pop();
                current_scissor = scissor_stack.iter().cloned().reduce(intersect);
            }
            DisplayItem::Opacity { alpha } => {
                current_opacity = *alpha;
            }
            DisplayItem::Text { .. } => { /* handled separately by text subsystem */ }
        }
    }
    if !current_quads.is_empty() {
        batches.push(Batch {
            scissor: current_scissor,
            quads: current_quads,
        });
    }
    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensure diff reports NoChange for identical lists and ReplaceAll for changes.
    #[test]
    fn diff_basic() {
        let mut a = DisplayList::new();
        a.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            color: [1.0, 0.0, 0.0, 1.0],
        });
        let mut b = a.clone();
        assert_eq!(a.diff(&b), DisplayListDiff::NoChange);
        b.push(DisplayItem::Text {
            x: 5.0,
            y: 5.0,
            text: "hi".to_string(),
            color: [0.0, 0.0, 0.0],
            font_size: 12.0,
            bounds: None,
        });
        match a.diff(&b) {
            DisplayListDiff::ReplaceAll(items) => assert_eq!(items.len(), 2),
            other => panic!("unexpected diff: {other:?}"),
        }
    }

    /// Ensure flattening collects rects and text correctly.
    #[test]
    fn flatten_basic() {
        let list = DisplayList::from_items(vec![
            DisplayItem::Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
                color: [0.5, 0.5, 0.5, 1.0],
            },
            DisplayItem::Text {
                x: 10.0,
                y: 20.0,
                text: "abc".into(),
                color: [0.0, 0.0, 0.0],
                font_size: 14.0,
                bounds: None,
            },
        ]);
        let (rects, texts) = list.flatten_to_immediate();
        assert_eq!(rects.len(), 1);
        assert_eq!(texts.len(), 1);
        assert_eq!(rects[0].width, 3.0);
        assert_eq!(texts[0].text, "abc");
    }

    /// Verify that clip scopes create separate batches with the correct scissor rects.
    #[test]
    fn batching_with_clips() {
        let list = DisplayList::from_items(vec![
            DisplayItem::Rect {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 10.0,
                color: [1.0, 0.0, 0.0, 1.0],
            },
            DisplayItem::BeginClip {
                x: 5.0,
                y: 5.0,
                width: 10.0,
                height: 10.0,
            },
            DisplayItem::Rect {
                x: 6.0,
                y: 6.0,
                width: 2.0,
                height: 2.0,
                color: [0.0, 1.0, 0.0, 1.0],
            },
            DisplayItem::EndClip,
        ]);
        let batches = batch_display_list(&list, 100, 100);
        assert_eq!(
            batches.len(),
            2,
            "expected two batches: pre-clip and clipped"
        );
        assert!(batches[0].scissor.is_none(), "first batch has no scissor");
        assert!(!batches[0].quads.is_empty());
        assert_eq!(batches[1].scissor, Some((5, 5, 10, 10)));
        assert_eq!(batches[1].quads.len(), 1);
    }
}
