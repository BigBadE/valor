//! Retained display list (DL) primitives and a minimal diffing API.
//!
//! Phase 6 introduces a GPU-friendly retained display list so that we can:
//! - Build paint commands from layout once per frame (or just for dirty regions),
//! - Diff two lists and submit only minimal updates to the renderer, and
//! - Keep a stable, engine-agnostic representation for later compositing.

use crate::renderer::{DrawRect, DrawText};
use core::mem::take;

/// Framebuffer pixel bounds for text (left, top, right, bottom).
pub type TextBoundsPx = (i32, i32, i32, i32);

/// Compact aliases to keep tuple-heavy types readable and satisfy clippy's `type_complexity`.
pub type Scissor = (u32, u32, u32, u32);

/// Gradient color stops as (offset, color) pairs, where offset is in [0,1].
pub type GradientStops = Vec<(f32, [f32; 4])>;

/// Stacking context boundary markers for proper opacity grouping.
/// Spec: CSS 2.2 §9.9.1 - Stacking contexts
/// Spec: CSS Compositing Level 1 §3.1 - Stacking context creation
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StackingContextBoundary {
    /// Opacity less than 1.0 creates a stacking context
    /// Spec: <https://www.w3.org/TR/CSS22/zindex.html#stacking-context>
    Opacity { alpha: f32 },
    /// 3D transforms create stacking contexts
    /// Spec: <https://www.w3.org/TR/css-transforms-1/#stacking-context>
    Transform { matrix: [f32; 16] },
    /// CSS filters create stacking contexts
    /// Spec: <https://www.w3.org/TR/filter-effects-1/#FilterProperty>
    Filter { filter_id: u32 },
    /// Isolation property creates stacking contexts
    /// Spec: <https://www.w3.org/TR/css-compositing-1/#isolation>
    Isolation,
    /// Positioned elements with z-index create stacking contexts
    /// Spec: <https://www.w3.org/TR/CSS22/zindex.html#stacking-context>
    ZIndex { z_index: i32 },
}

/// A single display list item.
/// This MVP focuses on rectangles and text, with lightweight placeholders for
/// clips and opacity that can be wired up later without breaking the API.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
        /// Requested font weight from CSS (100-900, default 400 = normal, 700 = bold)
        font_weight: u16,
        /// Matched font weight after CSS font matching (e.g., requested 300 -> matched 400)
        matched_font_weight: u16,
        /// Font family (e.g., "Courier New", "monospace")
        font_family: Option<String>,
        /// Line height in pixels (for vertical metrics) - ROUNDED for layout
        line_height: f32,
        /// Unrounded line height in pixels - for rendering to match layout calculations
        line_height_unrounded: f32,
        /// Optional bounds for wrapping/clipping in framebuffer pixels.
        bounds: Option<TextBoundsPx>,
        /// Measured text width from layout (for wrapping during rendering)
        measured_width: f32,
    },
    /// Push a rectangular clip onto the stack (placeholder; enforced as a scissor in `RenderState` for now).
    BeginClip {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// Pop the most recent clip from the stack.
    EndClip,
    /// Begin a stacking context boundary (replaces old paired Opacity)
    /// Spec: CSS 2.2 §9.9.1 - Elements that establish stacking contexts
    BeginStackingContext { boundary: StackingContextBoundary },
    /// End the current stacking context (implicit - marks end of grouped content)
    EndStackingContext,
    /// CSS border with optional border-radius.
    /// Spec: CSS Backgrounds and Borders Module Level 3
    /// <https://www.w3.org/TR/css-backgrounds-3/#borders>
    Border {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_width: f32,
        border_color: [f32; 4],
        /// Single radius for MVP, can expand to per-corner radii later
        border_radius: f32,
    },
    /// CSS box-shadow.
    /// Spec: CSS Backgrounds and Borders Module Level 3
    /// <https://www.w3.org/TR/css-backgrounds-3/#box-shadow>
    BoxShadow {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
        spread_radius: f32,
        color: [f32; 4],
    },
    /// Image rendering (background or content images).
    /// Spec: CSS Images Module Level 3
    /// <https://www.w3.org/TR/css-images-3/>
    Image {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        /// Reference to loaded image in resource pool
        image_id: u32,
    },
    /// Linear gradient.
    /// Spec: CSS Images Module Level 3 - Linear Gradients
    /// <https://www.w3.org/TR/css-images-3/#linear-gradients>
    LinearGradient {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        /// Gradient angle in radians (0 = horizontal, π/2 = vertical)
        angle: f32,
        /// Color stops as (offset, color) pairs, offset in [0,1]
        stops: GradientStops,
    },
    /// Radial gradient.
    /// Spec: CSS Images Module Level 3 - Radial Gradients
    /// <https://www.w3.org/TR/css-images-3/#radial-gradients>
    RadialGradient {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        /// Color stops as (offset, color) pairs, offset in [0,1]
        stops: GradientStops,
    },
}

impl Default for DisplayList {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// A retained display list with a monotonically increasing generation counter.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DisplayList {
    /// Linear sequence of display items to be drawn in order.
    pub items: Vec<DisplayItem>,
    /// Generation tag for debugging and quick equality checks across frames.
    pub generation: u64,
}

impl DisplayList {
    /// Create a new empty display list.
    #[inline]
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            generation: 0,
        }
    }

    /// Create a display list from an iterator of items.
    #[inline]
    pub fn from_items<I: IntoIterator<Item = DisplayItem>>(items: I) -> Self {
        let mut list = Self::new();
        list.items.extend(items);
        list
    }

    /// Bump the generation counter and return the new value.
    #[inline]
    pub const fn bump_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    /// Append an item to the end of the list.
    #[inline]
    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    /// Return a simple diff between two display lists.
    ///
    /// MVP strategy: if the lists are exactly equal, return `NoChange`; otherwise
    /// request a `ReplaceAll` with the target items. This keeps the API stable for
    /// future fine-grained diffs without overengineering now.
    #[inline]
    pub fn diff(&self, other: &Self) -> DisplayListDiff {
        if self == other {
            DisplayListDiff::NoChange
        } else {
            DisplayListDiff::ReplaceAll(other.items.clone())
        }
    }

    /// Flatten the display list to immediate-mode draw calls.
    #[inline]
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
                    });
                }
                DisplayItem::Text {
                    x,
                    y,
                    text,
                    color,
                    font_size,
                    font_weight,
                    matched_font_weight,
                    font_family,
                    line_height,
                    line_height_unrounded,
                    bounds,
                    measured_width,
                } => texts.push(DrawText {
                    x: *x,
                    y: *y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    font_weight: *font_weight,
                    matched_font_weight: *matched_font_weight,
                    font_family: font_family.clone(),
                    line_height: *line_height,
                    line_height_unrounded: *line_height_unrounded,
                    bounds: *bounds,
                    measured_width: *measured_width,
                }),
                DisplayItem::BeginClip { .. }
                | DisplayItem::EndClip
                | DisplayItem::BeginStackingContext { .. }
                | DisplayItem::EndStackingContext
                | DisplayItem::Border { .. }
                | DisplayItem::BoxShadow { .. }
                | DisplayItem::Image { .. }
                | DisplayItem::LinearGradient { .. }
                | DisplayItem::RadialGradient { .. } => {
                    // Placeholders for future stateful rendering path.
                    // These items require specialized rendering pipelines not yet
                    // supported by the immediate-mode path.
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

/// Convert a float rectangle to a scissor rectangle in framebuffer coordinates.
fn rect_to_scissor(rect: (f32, f32, f32, f32), framebuffer_size: (u32, u32)) -> Scissor {
    let (rect_x, rect_y, width, height) = rect;
    let (framebuffer_width, framebuffer_height) = framebuffer_size;
    let framebuffer_w = framebuffer_width.max(1);
    let framebuffer_h = framebuffer_height.max(1);
    let mut scissor_x = rect_x.max(0.0).floor() as i32;
    let mut scissor_y = rect_y.max(0.0).floor() as i32;
    let mut scissor_width = width.max(0.0).ceil() as i32;
    let mut scissor_height = height.max(0.0).ceil() as i32;
    if scissor_x < 0i32 {
        scissor_width += scissor_x;
        scissor_x = 0i32;
    }
    if scissor_y < 0i32 {
        scissor_height += scissor_y;
        scissor_y = 0i32;
    }
    let max_w = i32::try_from(framebuffer_w).unwrap_or(i32::MAX) - scissor_x;
    let max_h = i32::try_from(framebuffer_h).unwrap_or(i32::MAX) - scissor_y;
    let final_width = if max_w <= 0 { 0 } else { u32::try_from(scissor_width.clamp(0i32, max_w)).unwrap_or(0) };
    let final_height = if max_h <= 0 { 0 } else { u32::try_from(scissor_height.clamp(0i32, max_h)).unwrap_or(0) };
    (
        u32::try_from(scissor_x).unwrap_or(0),
        u32::try_from(scissor_y).unwrap_or(0),
        final_width,
        final_height,
    )
}

/// Intersect two scissor rectangles.
fn intersect_scissors(scissor_a: Scissor, scissor_b: Scissor) -> Scissor {
    let (a_x, a_y, a_width, a_height) = scissor_a;
    let (b_x, b_y, b_width, b_height) = scissor_b;
    let left = a_x.max(b_x);
    let top = a_y.max(b_y);
    let right = (a_x + a_width).min(b_x + b_width);
    let bottom = (a_y + a_height).min(b_y + b_height);
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    (left, top, width, height)
}

/// Helper to flush current quads into a batch if non-empty.
fn flush_batch(
    batches: &mut Vec<Batch>,
    current_quads: &mut Vec<Quad>,
    current_scissor: Option<Scissor>,
) {
    if !current_quads.is_empty() {
        batches.push(Batch {
            scissor: current_scissor,
            quads: take(current_quads),
        });
    }
}

/// Compute batches from a `DisplayList` by segmenting on clip stack boundaries.
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

    for item in &list.items {
        match item {
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                color,
            } => {
                let rgba = *color;
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
                flush_batch(&mut batches, &mut current_quads, current_scissor);
                let new_scissor = rect_to_scissor(
                    (*x, *y, *width, *height),
                    (framebuffer_width, framebuffer_height),
                );
                let effective_scissor = current_scissor.map_or(new_scissor, |parent| {
                    intersect_scissors(parent, new_scissor)
                });
                scissor_stack.push(effective_scissor);
                if current_scissor != Some(effective_scissor) {
                    current_scissor = Some(effective_scissor);
                }
            }
            DisplayItem::EndClip => {
                flush_batch(&mut batches, &mut current_quads, current_scissor);
                let _: Option<Scissor> = scissor_stack.pop();
                current_scissor = scissor_stack.iter().copied().reduce(intersect_scissors);
            }
            DisplayItem::BeginStackingContext { .. }
            | DisplayItem::EndStackingContext
            | DisplayItem::Text { .. }
            | DisplayItem::Border { .. }
            | DisplayItem::BoxShadow { .. }
            | DisplayItem::Image { .. }
            | DisplayItem::LinearGradient { .. }
            | DisplayItem::RadialGradient { .. } => {
                /* handled at a higher level by stacking context processor, text subsystem, or specialized pipelines */
            }
        }
    }
    flush_batch(&mut batches, &mut current_quads, current_scissor);
    batches
}
