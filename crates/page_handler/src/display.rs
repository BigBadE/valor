use crate::snapshots::{IRect, SnapshotItem, SnapshotSlice};
use css::layout_helpers::{collapse_whitespace, reorder_bidi_for_display};
use css::style_types::{BorderStyle, ComputedStyle, Overflow, Position};
use css_core::{LayoutNodeKind, LayoutRect};
use js::NodeKey;
use log::{debug, warn};
use renderer::StackingContextBoundary;
use renderer::{DisplayItem, DisplayList};
use std::collections::HashMap;

// Text shaping imports for measuring and line breaking
use glyphon::{
    Attrs as GlyphonAttrs, Buffer as GlyphonBuffer, FontSystem as GlyphonFontSystem,
    Metrics as GlyphonMetrics, Shaping as GlyphonShaping,
};
use std::sync::{LazyLock, Mutex};
use unicode_linebreak::{BreakOpportunity, linebreaks};

/// Walker context bundling borrowed maps for shallow recursion helpers.
///
/// This struct aggregates all the state needed during display list generation
/// to avoid passing many individual parameters through recursive calls.
struct WalkCtx<'context> {
    /// Maps node keys to their layout kind (`Document`, `Block`, `InlineText`).
    kind_map: &'context HashMap<NodeKey, LayoutNodeKind>,

    /// Maps parent node keys to their children.
    children_map: &'context HashMap<NodeKey, Vec<NodeKey>>,

    /// Maps node keys to their computed layout rectangles.
    rects: &'context HashMap<NodeKey, LayoutRect>,

    /// Primary map of computed styles from the CSS engine.
    computed_map: &'context HashMap<NodeKey, ComputedStyle>,

    /// Fallback computed styles for nodes missing primary styles.
    computed_fallback: &'context HashMap<NodeKey, ComputedStyle>,

    /// Optional robust computed styles (third-tier fallback).
    computed_robust: &'context Option<HashMap<NodeKey, ComputedStyle>>,

    /// Maps child node keys to their parent for upward traversal.
    parent_map: &'context HashMap<NodeKey, NodeKey>,
}

/// Determines if a computed style establishes a stacking context boundary.
///
/// Returns `Some` with the appropriate boundary type if the style creates
/// a stacking context via opacity or positioned z-index.
#[must_use]
fn stacking_boundary_for(computed_style: &ComputedStyle) -> Option<StackingContextBoundary> {
    if let Some(alpha) = computed_style.opacity
        && alpha < 1.0
    {
        return Some(StackingContextBoundary::Opacity { alpha });
    }
    if !matches!(computed_style.position, Position::Static)
        && let Some(z_index) = computed_style.z_index
    {
        return Some(StackingContextBoundary::ZIndex { z_index });
    }
    None
}

/// Input data retained between frames for incremental display list generation.
///
/// This structure aggregates all the state needed to build a display list,
/// including layout rectangles, computed styles, and UI overlays.
pub struct RetainedInputs {
    /// Layout rectangles for each node in the tree.
    pub rects: HashMap<NodeKey, LayoutRect>,

    /// Snapshot of the layout tree structure.
    pub snapshot: Vec<SnapshotItem>,

    /// Primary computed styles from the CSS engine.
    pub computed_map: HashMap<NodeKey, ComputedStyle>,

    /// Fallback computed styles for nodes missing primary styles.
    pub computed_fallback: HashMap<NodeKey, ComputedStyle>,

    /// Optional robust computed styles (third-tier fallback).
    pub computed_robust: Option<HashMap<NodeKey, ComputedStyle>>,

    /// Selection overlay rectangle (x0, y0, x1, y1) in screen coordinates.
    pub selection_overlay: Option<IRect>,

    /// Currently focused node for focus ring rendering.
    pub focused_node: Option<NodeKey>,

    /// Whether to render the heads-up display (HUD) with perf metrics.
    pub hud_enabled: bool,

    /// Number of deferred spillover operations (for HUD display).
    pub spillover_deferred: u64,

    /// Number of nodes restyled in the last style pass (for HUD display).
    pub last_style_restyled_nodes: u64,
}

/// Derives line metrics (ascent, descent, leading) from shaped text content.
///
/// Attempts to use real font metrics via glyphon shaping. Currently falls back
/// to heuristic ratios (80% ascent, 20% descent) when face metrics are unavailable.
///
/// Returns: `(ascent_px, descent_px, leading_px, optional_line_height)`
///
/// # Panics
///
/// Panics if the glyphon font system mutex is poisoned.
#[must_use]
fn derive_line_metrics_from_content(
    line_text: &str,
    font_size: f32,
) -> (f32, f32, f32, Option<f32>) {
    if line_text.is_empty() {
        // Nothing to measure; avoid shaping overhead.
        return (font_size * 0.8, font_size * 0.2, 0.0, None);
    }
    let mut font_system = GLYPHON_FONT_SYSTEM
        .lock()
        .expect("glyphon font system lock poisoned");
    let metrics = GlyphonMetrics::new(font_size, font_size);
    let mut buffer = GlyphonBuffer::new(&mut font_system, metrics);
    let attrs = GlyphonAttrs::new();
    buffer.set_text(
        &mut font_system,
        line_text,
        &attrs,
        GlyphonShaping::Advanced,
    );
    // Probe the first run/glyph to obtain the font_id being used for this content.
    // Keep this in case future versions allow resolving face metrics via font_id.
    let first_run = match buffer.layout_runs().next() {
        Some(run) => run,
        None => {
            warn!(
                "glyph metrics fallback: no runs; using heuristic; content='{line_text}' size={font_size}"
            );
            return (font_size * 0.8, font_size * 0.2, 0.0, None);
        }
    };
    let glyph = match first_run.glyphs.first() {
        Some(glyph_data) => glyph_data,
        None => {
            warn!(
                "glyph metrics fallback: no glyphs; using heuristic; content='{line_text}' size={font_size}"
            );
            return (font_size * 0.8, font_size * 0.2, 0.0, None);
        }
    };
    // We have access to glyph.font_id, but at this revision we compute used line-height
    // from the shaped glyph's optional line height when present, and fall back to heuristic
    // ascent/descent ratios. This keeps us consistent with the buffer's shaping.
    let used_line_height_opt = glyph.line_height_opt;
    let ascent_px = font_size * 0.8;
    let descent_px = font_size * 0.2;
    if used_line_height_opt.is_none() {
        warn!(
            "glyph metrics fallback: no glyph line_height; using heuristic; content='{line_text}' size={font_size}"
        );
    }
    let leading_px = used_line_height_opt
        .map(|line_height| (line_height - (ascent_px + descent_px)).max(0.0))
        .unwrap_or(0.0);
    (ascent_px, descent_px, leading_px, used_line_height_opt)
}

/// Shared glyphon `FontSystem` for text measurement and shaping.
static GLYPHON_FONT_SYSTEM: LazyLock<Mutex<GlyphonFontSystem>> =
    LazyLock::new(|| Mutex::new(GlyphonFontSystem::new()));

/// Measures the width of shaped text in pixels.
///
/// Uses glyphon to shape the text and sum the widths of all layout runs.
///
/// # Panics
///
/// Panics if the glyphon font system mutex is poisoned.
#[must_use]
fn measure_text_width_px(text: &str, font_size: f32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let mut font_system = GLYPHON_FONT_SYSTEM
        .lock()
        .expect("glyphon font system lock poisoned");
    let metrics = GlyphonMetrics::new(font_size, font_size);
    let mut buffer = GlyphonBuffer::new(&mut font_system, metrics);
    let attrs = GlyphonAttrs::new();
    buffer.set_text(&mut font_system, text, &attrs, GlyphonShaping::Advanced);
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .sum::<f32>()
        .round() as i32
}

/// Pushes border display items to the display list.
///
/// Builds and appends border rectangles for all four sides if the border is visible.
fn push_border_items(list: &mut DisplayList, rect: &LayoutRect, computed_style: &ComputedStyle) {
    if let Some(items) = build_border_items(rect, computed_style) {
        for item in items {
            list.push(item);
        }
    }
}

/// Builds border display items for a layout rectangle.
///
/// Returns `None` if the border is transparent or not solid.
/// Returns `Some` with up to 4 rectangles (top, right, bottom, left).
#[must_use]
fn build_border_items(
    rect: &LayoutRect,
    computed_style: &ComputedStyle,
) -> Option<Vec<DisplayItem>> {
    let border_width = computed_style.border_width;
    let border_style = computed_style.border_style;
    let border_color = computed_style.border_color;
    let color = [
        f32::from(border_color.red) / 255.0,
        f32::from(border_color.green) / 255.0,
        f32::from(border_color.blue) / 255.0,
        f32::from(border_color.alpha) / 255.0,
    ];
    if !(color[3] > 0.0 && matches!(border_style, BorderStyle::Solid)) {
        return None;
    }
    let rect_x = rect.x;
    let rect_y = rect.y;
    let rect_width = rect.width;
    let rect_height = rect.height;
    let top_width = border_width.top.max(0.0);
    let right_width = border_width.right.max(0.0);
    let bottom_width = border_width.bottom.max(0.0);
    let left_width = border_width.left.max(0.0);
    let mut items: Vec<DisplayItem> = Vec::with_capacity(4);
    if top_width > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect_x,
            y: rect_y,
            width: rect_width,
            height: top_width,
            color,
        });
    }
    if bottom_width > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect_x,
            y: rect_y + rect_height - bottom_width,
            width: rect_width,
            height: bottom_width,
            color,
        });
    }
    if left_width > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect_x,
            y: rect_y,
            width: left_width,
            height: rect_height,
            color,
        });
    }
    if right_width > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect_x + rect_width - right_width,
            y: rect_y,
            width: right_width,
            height: rect_height,
            color,
        });
    }
    Some(items)
}

/// Computes a z-index sort key for a child node.
///
/// Returns `(stacking_bucket, dom_order)` where:
/// - `stacking_bucket`: -2 for negative z-index, 0 for static, 1 for positioned auto/0, 2 for positive
/// - `dom_order`: the node's raw key value for stable sorting within buckets
///
/// Per CSS 2.2 paint order: negative z-index, normal flow, positioned auto/0, positive z-index.
#[must_use]
fn z_key_for_child(
    child: NodeKey,
    parent_map: &HashMap<NodeKey, NodeKey>,
    computed_map: &HashMap<NodeKey, ComputedStyle>,
    computed_fallback: &HashMap<NodeKey, ComputedStyle>,
    computed_robust: Option<&HashMap<NodeKey, ComputedStyle>>,
) -> (i32, u64) {
    // Inline nearest-style lookup to avoid deep nesting and inner function coupling
    let style = {
        let mut current = Some(child);
        let mut found: Option<&ComputedStyle> = None;
        while let Some(node_key) = current {
            if let Some(computed_style) = computed_robust
                .and_then(|map| map.get(&node_key))
                .or_else(|| computed_fallback.get(&node_key))
                .or_else(|| computed_map.get(&node_key))
            {
                found = Some(computed_style);
                break;
            }
            current = parent_map.get(&node_key).copied();
        }
        found
    };
    let (position, z_index_opt) = style
        .map(|computed_style| (computed_style.position, computed_style.z_index))
        .unwrap_or((Position::Static, None));
    // CSS 2.2 stacking order (simplified):
    //  - Negative z-index positioned descendants behind
    //  - Normal flow (non-positioned)
    //  - Positioned with z-index auto/0
    //  - Positive z-index positioned descendants on top
    let positioned = !matches!(position, Position::Static);
    let bucket: i32 = if positioned {
        match z_index_opt {
            Some(value) if value < 0 => -2,
            Some(value) if value > 0 => 2,
            Some(_) | None => 1,
        }
    } else {
        0
    };
    // Sort by bucket first, then DOM order within each bucket.
    (bucket, child.0)
}

/// Builder trait for constructing display lists from layout data.
///
/// Implementations can customize how layout information is transformed into
/// renderer-consumable display items, rectangles, and text runs.
pub trait DisplayBuilder: Send + Sync {
    /// Builds a complete display list from retained frame inputs.
    ///
    /// Processes layout rectangles, computed styles, and UI overlays to produce
    /// a hierarchical display list with stacking contexts and clipping regions.
    fn build_retained(&self, inputs: RetainedInputs) -> DisplayList;

    /// Builds a simple list of filled rectangles for debugging layout boxes.
    fn build_rect_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
    ) -> Vec<renderer::DrawRect>;

    /// Builds a list of text runs with font properties and clipping bounds.
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
        computed_map: &HashMap<NodeKey, ComputedStyle>,
    ) -> Vec<renderer::DrawText>;
}

/// Default implementation of `DisplayBuilder` using the module's display functions.
pub struct DefaultDisplayBuilder;

impl DisplayBuilder for DefaultDisplayBuilder {
    #[inline]
    fn build_retained(&self, inputs: RetainedInputs) -> DisplayList {
        build_retained(inputs)
    }

    #[inline]
    fn build_rect_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
    ) -> Vec<renderer::DrawRect> {
        build_rect_list(rects, snapshot)
    }

    #[inline]
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
        computed_map: &HashMap<NodeKey, ComputedStyle>,
    ) -> Vec<renderer::DrawText> {
        build_text_list(rects, snapshot, computed_map)
    }
}

/// Builds a simple list of white-filled rectangles for all block layout boxes.
///
/// Used for debugging and visualizing layout structure. Ignores inline text nodes.
#[must_use]
pub fn build_rect_list(
    rects: &HashMap<NodeKey, LayoutRect>,
    snapshot: SnapshotSlice,
) -> Vec<renderer::DrawRect> {
    let mut list: Vec<renderer::DrawRect> = Vec::new();
    for (node, kind, _children) in snapshot {
        if !matches!(kind, LayoutNodeKind::Block { .. }) {
            continue;
        }
        if let Some(rect) = rects.get(node) {
            list.push(renderer::DrawRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                color: [1.0, 1.0, 1.0],
            });
        }
    }
    list
}

/// Builds a list of text runs with proper line breaking, metrics, and clipping.
///
/// Processes inline text nodes from the layout snapshot, applying:
/// - Font size and color from computed styles
/// - Line breaking using Unicode `UAX#14`
/// - Overflow clipping to content-box bounds
/// - `BiDi` reordering for visual display
#[must_use]
pub fn build_text_list(
    rects: &HashMap<NodeKey, LayoutRect>,
    snapshot: SnapshotSlice,
    computed_map: &HashMap<NodeKey, ComputedStyle>,
) -> Vec<renderer::DrawText> {
    let mut list: Vec<renderer::DrawText> = Vec::new();
    // Build a parent map so inline text can inherit from its element parent
    let mut parent_of: HashMap<NodeKey, NodeKey> = HashMap::new();
    for (parent, _kind, children) in snapshot {
        for &child in children {
            parent_of.insert(child, *parent);
        }
    }
    // Helper: climb ancestors until a computed style is found
    let nearest_style = |start: NodeKey| -> Option<&ComputedStyle> {
        let mut current = Some(start);
        while let Some(node) = current {
            if let Some(computed_style) = computed_map.get(&node) {
                return Some(computed_style);
            }
            current = parent_of.get(&node).copied();
        }
        None
    };
    // Helper: climb ancestors until a rect is found (inline text nodes don't have rects).
    let nearest_rect = |start: NodeKey| -> Option<LayoutRect> {
        let mut current = Some(start);
        while let Some(node) = current {
            if let Some(rect) = rects.get(&node) {
                return Some(*rect);
            }
            current = parent_of.get(&node).copied();
        }
        None
    };
    for (key, kind, _children) in snapshot {
        if let LayoutNodeKind::InlineText { text } = kind {
            if text.trim().is_empty() {
                continue;
            }
            let rect = nearest_rect(*key)
                .or_else(|| nearest_rect(parent_of.get(key).copied().unwrap_or(*key)));
            if let Some(rect) = rect {
                let style_opt = nearest_style(*key);
                let (font_size, color_rgb) =
                    style_opt.map_or((16.0, [0.0, 0.0, 0.0]), |computed_style| {
                        let text_color = computed_style.color;
                        (
                            computed_style.font_size,
                            [
                                f32::from(text_color.red) / 255.0,
                                f32::from(text_color.green) / 255.0,
                                f32::from(text_color.blue) / 255.0,
                            ],
                        )
                    });
                let collapsed = collapse_whitespace(text);
                if collapsed.is_empty() {
                    continue;
                }
                // When overflow is hidden/clip, clip to the content-box width per CSS Display/Overflow.
                // Border-box rect is provided; compute content-box left and width.
                let (content_left_x, content_width_px) = style_opt
                    .filter(|computed_style| matches!(computed_style.overflow, Overflow::Hidden))
                    .map(|computed_style| {
                        let pad_left = computed_style.padding.left.max(0.0) as i32;
                        let pad_right = computed_style.padding.right.max(0.0) as i32;
                        let border_left = computed_style.border_width.left.max(0.0) as i32;
                        let border_right = computed_style.border_width.right.max(0.0) as i32;
                        let left_x = (rect.x.round() as i32) + border_left + pad_left;
                        let width_px = (rect.width.round() as i32)
                            .saturating_sub(border_left + pad_left + pad_right + border_right);
                        (left_x, width_px)
                    })
                    .unwrap_or(((rect.x.round() as i32), (rect.width.round() as i32)));
                let max_width_px = content_width_px.max(0);
                // Prefer computed line-height when available; otherwise use real metrics if available.
                let (ascent_px, descent_px, leading_px, line_height_from_glyph) =
                    derive_line_metrics_from_content(&collapsed, font_size);
                let computed_line_height =
                    style_opt.and_then(|computed_style| computed_style.line_height);
                let used_line_height = computed_line_height
                    .or(line_height_from_glyph)
                    .unwrap_or_else(|| {
                        // Spec-like normal using metrics; keep a minimum padding to avoid clip.
                        let sum = ascent_px + descent_px + leading_px;
                        sum.max(font_size + 2.0)
                    });
                let line_height = used_line_height.round() as i32;
                let ascent = ascent_px.round() as i32; // placeholder until face metrics are available
                let _descent = descent_px.round() as i32;
                let lines = wrap_text_uax14(&collapsed, font_size, max_width_px);
                for (line_index, raw_line) in lines.iter().enumerate() {
                    let visual_line = reorder_bidi_for_display(raw_line);
                    let line_top = (rect.y.round() as i32) + (line_index as i32) * line_height;
                    let baseline_y = line_top + ascent;
                    // Use line box bounds: top at line_top; bottom at line_top + line_height
                    let top = line_top;
                    let bottom = line_top + line_height;
                    let bounds = Some((content_left_x, top, content_left_x + max_width_px, bottom));
                    list.push(renderer::DrawText {
                        x: content_left_x as f32,
                        y: baseline_y as f32,
                        text: visual_line,
                        color: color_rgb,
                        font_size,
                        bounds,
                    });
                }
            }
        }
    }
    list
}

/// Pushes text display items to the display list with line breaking and `BiDi` reordering.
///
/// Used for immediate-mode text rendering without computed styles. Derives line
/// metrics from glyph shaping and wraps text using `UAX#14` line breaking.
fn push_text_item(
    list: &mut DisplayList,
    rect: &LayoutRect,
    text: &str,
    font_size: f32,
    color_rgb: [f32; 3],
) {
    let collapsed = collapse_whitespace(text);
    if collapsed.is_empty() {
        return;
    }
    let max_width_px = (rect.width.round() as i32).max(0);
    // Immediate path does not have computed styles; prefer glyph-provided line height.
    let (ascent_px, descent_px, leading_px, line_height_from_glyph) =
        derive_line_metrics_from_content(&collapsed, font_size);
    let used_line_height = line_height_from_glyph
        .unwrap_or_else(|| (ascent_px + descent_px + leading_px).max(font_size + 2.0));
    let line_height = used_line_height.round() as i32;
    let ascent = ascent_px.round() as i32; // placeholder until face metrics are available
    let _descent = descent_px.round() as i32;
    let broken_lines = wrap_text_uax14(&collapsed, font_size, max_width_px);
    for (line_index, raw_line) in broken_lines.iter().enumerate() {
        let visual_line = reorder_bidi_for_display(raw_line);
        let line_top = (rect.y.round() as i32) + (line_index as i32) * line_height;
        let baseline_y = line_top + ascent;
        let top = line_top;
        let bottom = line_top + line_height;
        let bounds = Some((
            rect.x.round() as i32,
            top,
            (rect.x + rect.width).round() as i32,
            bottom,
        ));
        list.push(DisplayItem::Text {
            x: rect.x,
            y: baseline_y as f32,
            text: visual_line,
            color: color_rgb,
            font_size,
            bounds,
        });
    }
}

/// Breaks text into lines using Unicode line breaking (UAX#14).
///
/// Greedily packs text runs while measuring shaped widths. This improves fidelity
/// for scripts where whitespace-only breaking is insufficient.
///
/// Returns a vector of line strings, trimming trailing spaces from each line.
///
/// # Panics
///
/// May panic if the `linebreaks` iterator produces indices that are not at UTF-8
/// character boundaries. This should not occur with well-formed text.
#[must_use]
fn wrap_text_uax14(text: &str, font_size: f32, max_width_px: i32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if max_width_px <= 0 || text.is_empty() {
        if !text.is_empty() {
            lines.push(text.to_string());
        }
        return lines;
    }
    let mut start = 0usize;
    let mut last_good = 0usize;
    for (index, opportunity) in linebreaks(text) {
        let is_break = matches!(
            opportunity,
            BreakOpportunity::Mandatory | BreakOpportunity::Allowed
        );
        if is_break {
            // Measure candidate slice
            let candidate = &text[start..index];
            let width = measure_text_width_px(candidate, font_size);
            if width <= max_width_px {
                last_good = index;
                continue;
            }
            // Emit last good (or force break at current if none)
            if last_good > start {
                let mut slice = text[start..last_good].to_string();
                // Trim trailing spaces at line end
                while slice.ends_with(' ') {
                    slice.pop();
                }
                if !slice.is_empty() {
                    lines.push(slice);
                }
                start = last_good;
            } else {
                // Force character-level break to avoid infinite loop
                let end = index.max(start + 1);
                let mut slice = text[start..end].to_string();
                while slice.ends_with(' ') {
                    slice.pop();
                }
                if !slice.is_empty() {
                    lines.push(slice);
                }
                start = end;
                last_good = start;
            }
        }
    }
    // Remainder
    if start < text.len() {
        let mut tail = text[start..].to_string();
        while tail.ends_with(' ') {
            tail.pop();
        }
        if !tail.is_empty() {
            lines.push(tail);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Builds a complete display list from retained layout inputs.
///
/// Traverses the layout tree depth-first, emitting display items for:
/// - Background fills and borders
/// - Text runs with line breaking
/// - Stacking contexts (opacity, z-index)
/// - Clipping regions (overflow: hidden/clip)
/// - UI overlays (selection, focus rings, HUD)
///
/// Returns a hierarchical display list ready for GPU rendering.
#[must_use]
pub fn build_retained(inputs: RetainedInputs) -> DisplayList {
    // Helper: finds nearest ancestor with a computed style (for inline text inheritance).
    fn nearest_style<'style>(
        start: NodeKey,
        parent_map: &HashMap<NodeKey, NodeKey>,
        computed_map: &'style HashMap<NodeKey, ComputedStyle>,
        computed_fallback: &'style HashMap<NodeKey, ComputedStyle>,
        computed_robust: Option<&'style HashMap<NodeKey, ComputedStyle>>,
    ) -> Option<&'style ComputedStyle> {
        let mut current = Some(start);
        while let Some(node) = current {
            if let Some(computed_style) = computed_robust
                .and_then(|map| map.get(&node))
                .or_else(|| computed_fallback.get(&node))
                .or_else(|| computed_map.get(&node))
            {
                return Some(computed_style);
            }
            current = parent_map.get(&node).copied();
        }
        None
    }

    // Helper: finds nearest ancestor with a layout rect (inline text nodes lack rects).
    #[inline]
    fn nearest_rect(
        start: NodeKey,
        parent_map: &HashMap<NodeKey, NodeKey>,
        rects: &HashMap<NodeKey, LayoutRect>,
    ) -> Option<LayoutRect> {
        let mut current = Some(start);
        while let Some(node) = current {
            if let Some(rect) = rects.get(&node) {
                return Some(*rect);
            }
            current = parent_map.get(&node).copied();
        }
        None
    }

    // Helper: orders children by z-index stacking buckets for correct paint order.
    #[inline]
    fn order_children(
        children: &[NodeKey],
        parent_map: &HashMap<NodeKey, NodeKey>,
        computed_map: &HashMap<NodeKey, ComputedStyle>,
        computed_fallback: &HashMap<NodeKey, ComputedStyle>,
        computed_robust: Option<&HashMap<NodeKey, ComputedStyle>>,
    ) -> Vec<NodeKey> {
        let mut ordered: Vec<NodeKey> = children.to_vec();
        ordered.sort_by_key(|child| {
            let (bucket, dom_order) = z_key_for_child(
                *child,
                parent_map,
                computed_map,
                computed_fallback,
                computed_robust,
            );
            (bucket, dom_order)
        });
        ordered
    }

    // Helper: processes children in z-index paint order.
    #[inline]
    fn process_children(list: &mut DisplayList, node: NodeKey, ctx: &WalkCtx<'_>) {
        if let Some(children) = ctx.children_map.get(&node) {
            let ordered = order_children(
                children,
                ctx.parent_map,
                ctx.computed_map,
                ctx.computed_fallback,
                ctx.computed_robust.as_ref(),
            );
            for child in ordered {
                recurse(list, child, ctx);
            }
        }
    }

    let RetainedInputs {
        rects,
        snapshot,
        computed_map,
        computed_fallback,
        computed_robust,
        selection_overlay,
        focused_node,
        hud_enabled,
        spillover_deferred,
        last_style_restyled_nodes,
    } = inputs;

    let mut kind_map: HashMap<NodeKey, LayoutNodeKind> = HashMap::new();
    let mut children_map: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for (key, kind, children) in snapshot {
        kind_map.insert(key, kind);
        children_map.insert(key, children);
    }
    // Parent map for inheritance fallbacks
    let mut parent_map: HashMap<NodeKey, NodeKey> = HashMap::new();
    for (parent, children) in &children_map {
        for &child in children {
            parent_map.insert(child, *parent);
        }
    }

    // Recursive tree walker that emits display items for each node.
    fn recurse(list: &mut DisplayList, node: NodeKey, ctx: &WalkCtx<'_>) {
        let kind = match ctx.kind_map.get(&node) {
            Some(layout_kind) => layout_kind,
            None => return,
        };
        match kind {
            LayoutNodeKind::Document => {
                if let Some(children) = ctx.children_map.get(&node) {
                    for &child in children {
                        recurse(list, child, ctx);
                    }
                }
            }
            LayoutNodeKind::Block { .. } => {
                // Background/border/clip only if we have a rect for this node
                let rect_opt = ctx.rects.get(&node);
                let computed_style_opt = ctx
                    .computed_robust
                    .as_ref()
                    .and_then(|map| map.get(&node))
                    .or_else(|| ctx.computed_fallback.get(&node))
                    .or_else(|| ctx.computed_map.get(&node));
                if let Some(rect) = rect_opt {
                    // Determine if this node establishes a stacking context.
                    // Preference order: Opacity < 1.0, otherwise positioned with non-auto z-index.
                    let style_for_node_ctx = computed_style_opt;
                    let mut opened_ctx = false;
                    let boundary_opt = style_for_node_ctx.and_then(stacking_boundary_for);
                    if let Some(boundary) = boundary_opt {
                        list.push(DisplayItem::BeginStackingContext { boundary });
                        opened_ctx = true;
                    }
                    // Background fill from computed styles; only paint if non-transparent
                    let fill_rgba_opt = computed_style_opt.map(|computed_style| {
                        let background = computed_style.background_color;
                        [
                            f32::from(background.red) / 255.0,
                            f32::from(background.green) / 255.0,
                            f32::from(background.blue) / 255.0,
                            f32::from(background.alpha) / 255.0,
                        ]
                    });
                    if let Some(fill_rgba) = fill_rgba_opt.filter(|rgba| rgba[3] > 0.0) {
                        list.push(DisplayItem::Rect {
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            color: fill_rgba,
                        });
                    }
                    // Borders
                    if let Some(computed_style) = computed_style_opt {
                        push_border_items(list, rect, computed_style);
                    }
                    // overflow clip
                    let style_for_node = computed_style_opt.or_else(|| {
                        nearest_style(
                            node,
                            ctx.parent_map,
                            ctx.computed_map,
                            ctx.computed_fallback,
                            ctx.computed_robust.as_ref(),
                        )
                    });
                    let mut opened_clip = false;
                    if let Some(computed_style) = style_for_node
                        && matches!(
                            computed_style.overflow,
                            Overflow::Hidden | Overflow::Clip | Overflow::Auto | Overflow::Scroll
                        )
                    {
                        // Clip at the padding box per CSS Overflow spec. Compute padding-box
                        // from the border-box rect and the border widths.
                        let pad_left = computed_style.padding.left.max(0.0);
                        let pad_top = computed_style.padding.top.max(0.0);
                        let pad_right = computed_style.padding.right.max(0.0);
                        let pad_bottom = computed_style.padding.bottom.max(0.0);
                        let border_left = computed_style.border_width.left.max(0.0);
                        let border_top = computed_style.border_width.top.max(0.0);
                        let border_right = computed_style.border_width.right.max(0.0);
                        let border_bottom = computed_style.border_width.bottom.max(0.0);
                        let clip_x = rect.x + border_left + pad_left;
                        let clip_y = rect.y + border_top + pad_top;
                        let clip_width = (rect.width
                            - (border_left + pad_left + pad_right + border_right))
                            .max(0.0);
                        let clip_height = (rect.height
                            - (border_top + pad_top + pad_bottom + border_bottom))
                            .max(0.0);
                        list.push(DisplayItem::BeginClip {
                            x: clip_x,
                            y: clip_y,
                            width: clip_width,
                            height: clip_height,
                        });
                        opened_clip = true;
                    }
                    process_children(list, node, ctx);
                    // If a clip was opened inside this node, it must be closed
                    // before the stacking context ends so the clip applies to
                    // the entire context's painted content.
                    if opened_clip {
                        list.push(DisplayItem::EndClip);
                    }
                    if opened_ctx {
                        list.push(DisplayItem::EndStackingContext);
                    }
                } else {
                    // No rect for this block: still recurse into children; apply stacking context if present
                    let style_for_node_ctx = computed_style_opt.or_else(|| {
                        nearest_style(
                            node,
                            ctx.parent_map,
                            ctx.computed_map,
                            ctx.computed_fallback,
                            ctx.computed_robust.as_ref(),
                        )
                    });
                    let mut opened_ctx = false;
                    let boundary_opt = style_for_node_ctx.and_then(stacking_boundary_for);
                    if let Some(boundary) = boundary_opt {
                        list.push(DisplayItem::BeginStackingContext { boundary });
                        opened_ctx = true;
                    }
                    process_children(list, node, ctx);
                    if opened_ctx {
                        list.push(DisplayItem::EndStackingContext);
                    }
                }
            }
            LayoutNodeKind::InlineText { text } => {
                if text.trim().is_empty() {
                    return;
                }
                if let Some(rect) = nearest_rect(node, ctx.parent_map, ctx.rects) {
                    let (font_size, color_rgb) = nearest_style(
                        node,
                        ctx.parent_map,
                        ctx.computed_map,
                        ctx.computed_fallback,
                        ctx.computed_robust.as_ref(),
                    )
                    .map_or((16.0, [0.0, 0.0, 0.0]), |computed_style| {
                        let text_color = computed_style.color;
                        (
                            computed_style.font_size,
                            [
                                f32::from(text_color.red) / 255.0,
                                f32::from(text_color.green) / 255.0,
                                f32::from(text_color.blue) / 255.0,
                            ],
                        )
                    });
                    push_text_item(list, &rect, text, font_size, color_rgb);
                }
            }
        }
    }

    let mut list = DisplayList::new();
    let ctx = WalkCtx {
        kind_map: &kind_map,
        children_map: &children_map,
        rects: &rects,
        computed_map: &computed_map,
        computed_fallback: &computed_fallback,
        computed_robust: &computed_robust,
        parent_map: &parent_map,
    };
    recurse(&mut list, NodeKey::ROOT, &ctx);
    debug!(
        "[DL DEBUG] build_retained produced items={}",
        list.items.len()
    );

    // Render selection overlay as semi-transparent blue rectangles intersecting layout boxes
    if let Some((x0_coord, y0_coord, x1_coord, y1_coord)) = selection_overlay {
        let selection_x = x0_coord.min(x1_coord) as f32;
        let selection_y = y0_coord.min(y1_coord) as f32;
        let selection_width = (x0_coord.max(x1_coord) - selection_x.round() as i32).max(0) as f32;
        let selection_height = (y0_coord.max(y1_coord) - selection_y.round() as i32).max(0) as f32;
        let selection = LayoutRect {
            x: selection_x,
            y: selection_y,
            width: selection_width,
            height: selection_height,
        };
        for (_node_key, rect) in &rects {
            let intersect_x = rect.x.max(selection.x);
            let intersect_y = rect.y.max(selection.y);
            let intersect_right = (rect.x + rect.width).min(selection.x + selection.width);
            let intersect_bottom = (rect.y + rect.height).min(selection.y + selection.height);
            let intersect_width = (intersect_right - intersect_x).max(0.0);
            let intersect_height = (intersect_bottom - intersect_y).max(0.0);
            if intersect_width > 0.0 && intersect_height > 0.0 {
                list.push(DisplayItem::Rect {
                    x: intersect_x,
                    y: intersect_y,
                    width: intersect_width,
                    height: intersect_height,
                    color: [0.2, 0.5, 1.0, 0.35],
                });
            }
        }
    }

    // Render focus ring as a 4-sided border around the focused element
    if let Some(focused) = focused_node
        && let Some(focused_rect) = rects.get(&focused)
    {
        let focus_x = focused_rect.x;
        let focus_y = focused_rect.y;
        let focus_width = focused_rect.width;
        let focus_height = focused_rect.height;
        let focus_color = [0.2, 0.4, 1.0, 1.0];
        let focus_thickness = 2.0f32;
        // Top border
        list.push(DisplayItem::Rect {
            x: focus_x,
            y: focus_y,
            width: focus_width,
            height: focus_thickness,
            color: focus_color,
        });
        // Bottom border
        list.push(DisplayItem::Rect {
            x: focus_x,
            y: focus_y + focus_height - focus_thickness,
            width: focus_width,
            height: focus_thickness,
            color: focus_color,
        });
        // Left border
        list.push(DisplayItem::Rect {
            x: focus_x,
            y: focus_y,
            width: focus_thickness,
            height: focus_height,
            color: focus_color,
        });
        // Right border
        list.push(DisplayItem::Rect {
            x: focus_x + focus_width - focus_thickness,
            y: focus_y,
            width: focus_thickness,
            height: focus_height,
            color: focus_color,
        });
    }

    if hud_enabled {
        let hud = format!("restyled:{last_style_restyled_nodes} spill:{spillover_deferred}");
        list.push(DisplayItem::Text {
            x: 6.0,
            y: 14.0,
            text: hud,
            color: [0.1, 0.1, 0.1],
            font_size: 12.0,
            bounds: None,
        });
    }

    list
}
