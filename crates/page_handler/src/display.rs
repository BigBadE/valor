use crate::snapshots::IRect;
use css::layout_helpers::{collapse_whitespace, reorder_bidi_for_display};
use css::style_types::{BorderStyle, ComputedStyle, Overflow, Position};
use css_core::{LayoutNodeKind, LayoutRect};
use js::NodeKey;
use log::{debug, warn};
use std::collections::HashMap;
use wgpu_renderer::display_list::StackingContextBoundary;
use wgpu_renderer::{DisplayItem, DisplayList};

// Text shaping imports for measuring and line breaking
use glyphon::{
    Attrs as GlyphonAttrs, Buffer as GlyphonBuffer, FontSystem as GlyphonFontSystem,
    Metrics as GlyphonMetrics, Shaping as GlyphonShaping,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use unicode_linebreak::{BreakOpportunity, linebreaks};

// Module-scoped aliases to simplify complex tuple types
type SnapshotItem = (NodeKey, LayoutNodeKind, Vec<NodeKey>);
type SnapshotSlice<'a> = &'a [SnapshotItem];

// Walker context bundling borrowed maps for shallow recursion helpers
struct WalkCtx<'a> {
    kind_map: &'a HashMap<NodeKey, LayoutNodeKind>,
    children_map: &'a HashMap<NodeKey, Vec<NodeKey>>,
    rects: &'a HashMap<NodeKey, LayoutRect>,
    computed_map: &'a HashMap<NodeKey, ComputedStyle>,
    computed_fallback: &'a HashMap<NodeKey, ComputedStyle>,
    computed_robust: &'a Option<HashMap<NodeKey, ComputedStyle>>,
    parent_map: &'a HashMap<NodeKey, NodeKey>,
}

#[inline]
fn stacking_boundary_for(cs: &ComputedStyle) -> Option<StackingContextBoundary> {
    if let Some(alpha) = cs.opacity
        && alpha < 1.0
    {
        return Some(StackingContextBoundary::Opacity { alpha });
    }
    if !matches!(cs.position, Position::Static)
        && let Some(z) = cs.z_index
    {
        return Some(StackingContextBoundary::ZIndex { z });
    }
    None
}

pub struct RetainedInputs {
    pub rects: HashMap<NodeKey, LayoutRect>,
    pub snapshot: Vec<SnapshotItem>,
    pub computed_map: HashMap<NodeKey, ComputedStyle>,
    pub computed_fallback: HashMap<NodeKey, ComputedStyle>,
    pub computed_robust: Option<HashMap<NodeKey, ComputedStyle>>,
    pub selection_overlay: Option<IRect>,
    pub focused_node: Option<NodeKey>,
    pub hud_enabled: bool,
    pub spillover_deferred: u64,
    pub last_style_restyled_nodes: u64,
}

// Attempt to derive ascent/descent from the actual shaped line content. At this pinned
// glyphon/cosmic-text revision, we can access run glyphs and their cache keys to obtain
// a font_id; however, resolving that id into per-face metrics is not available via a stable
// public API here. As a result, we currently fall back to a safe heuristic while keeping the
// shaping of the real line content in one place to swap in true metrics when available.
#[inline]
fn derive_line_metrics_from_content(
    line_text: &str,
    font_size: f32,
) -> (f32, f32, f32, Option<f32>) {
    if line_text.is_empty() {
        // Nothing to measure; avoid shaping overhead.
        return (font_size * 0.8, font_size * 0.2, 0.0, None);
    }
    let mut fs = GLYPHON_FONT_SYSTEM
        .lock()
        .expect("glyphon font system lock poisoned");
    let metrics = GlyphonMetrics::new(font_size, font_size);
    let mut buffer = GlyphonBuffer::new(&mut fs, metrics);
    let attrs = GlyphonAttrs::new();
    buffer.set_text(&mut fs, line_text, &attrs, GlyphonShaping::Advanced);
    // Probe the first run/glyph to obtain the font_id being used for this content.
    // Keep this in case future versions allow resolving face metrics via font_id.
    let first_run = match buffer.layout_runs().next() {
        Some(r) => r,
        None => {
            warn!(
                "glyph metrics fallback: no runs; using heuristic; content='{line_text}' size={font_size}"
            );
            return (font_size * 0.8, font_size * 0.2, 0.0, None);
        }
    };
    let glyph = match first_run.glyphs.first() {
        Some(g) => g,
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
        .map(|lh| (lh - (ascent_px + descent_px)).max(0.0))
        .unwrap_or(0.0);
    (ascent_px, descent_px, leading_px, used_line_height_opt)
}

// Shared glyphon FontSystem for measurement
static GLYPHON_FONT_SYSTEM: Lazy<Mutex<GlyphonFontSystem>> =
    Lazy::new(|| Mutex::new(GlyphonFontSystem::new()));

#[inline]
fn measure_text_width_px(text: &str, font_size: f32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let mut fs = GLYPHON_FONT_SYSTEM
        .lock()
        .expect("glyphon font system lock poisoned");
    let metrics = GlyphonMetrics::new(font_size, font_size);
    let mut buffer = GlyphonBuffer::new(&mut fs, metrics);
    let attrs = GlyphonAttrs::new();
    buffer.set_text(&mut fs, text, &attrs, GlyphonShaping::Advanced);
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .sum::<f32>()
        .round() as i32
}

#[inline]
fn push_border_items(list: &mut DisplayList, rect: &LayoutRect, cs: &ComputedStyle) {
    if let Some(items) = build_border_items(rect, cs) {
        for item in items {
            list.push(item);
        }
    }
}

#[inline]
fn build_border_items(rect: &LayoutRect, cs: &ComputedStyle) -> Option<Vec<DisplayItem>> {
    let bw = cs.border_width;
    let bs = cs.border_style;
    let bc = cs.border_color;
    let color = [
        bc.red as f32 / 255.0,
        bc.green as f32 / 255.0,
        bc.blue as f32 / 255.0,
        bc.alpha as f32 / 255.0,
    ];
    if !(color[3] > 0.0 && matches!(bs, BorderStyle::Solid)) {
        return None;
    }
    let x = rect.x;
    let y = rect.y;
    let w = rect.width;
    let h = rect.height;
    let t = bw.top.max(0.0);
    let r = bw.right.max(0.0);
    let b = bw.bottom.max(0.0);
    let l = bw.left.max(0.0);
    let mut items: Vec<DisplayItem> = Vec::with_capacity(4);
    if t > 0.0 {
        items.push(DisplayItem::Rect {
            x,
            y,
            width: w,
            height: t,
            color,
        });
    }
    if b > 0.0 {
        items.push(DisplayItem::Rect {
            x,
            y: y + h - b,
            width: w,
            height: b,
            color,
        });
    }
    if l > 0.0 {
        items.push(DisplayItem::Rect {
            x,
            y,
            width: l,
            height: h,
            color,
        });
    }
    if r > 0.0 {
        items.push(DisplayItem::Rect {
            x: x + w - r,
            y,
            width: r,
            height: h,
            color,
        });
    }
    Some(items)
}

#[inline]
fn z_key_for_child(
    child: NodeKey,
    parent_map: &HashMap<NodeKey, NodeKey>,
    computed_map: &HashMap<NodeKey, ComputedStyle>,
    computed_fallback: &HashMap<NodeKey, ComputedStyle>,
    computed_robust: &Option<HashMap<NodeKey, ComputedStyle>>,
) -> (i32, u64) {
    // Inline nearest-style lookup to avoid deep nesting and inner function coupling
    let style = {
        let mut current = Some(child);
        let mut found: Option<&ComputedStyle> = None;
        while let Some(nkey) = current {
            if let Some(cs) = computed_robust
                .as_ref()
                .and_then(|m| m.get(&nkey))
                .or_else(|| computed_fallback.get(&nkey))
                .or_else(|| computed_map.get(&nkey))
            {
                found = Some(cs);
                break;
            }
            current = parent_map.get(&nkey).copied();
        }
        found
    };
    let (pos, zi_opt) = style
        .map(|cs| (cs.position, cs.z_index))
        .unwrap_or((Position::Static, None));
    // CSS 2.2 stacking order (simplified):
    //  - Negative z-index positioned descendants behind
    //  - Normal flow (non-positioned)
    //  - Positioned with z-index auto/0
    //  - Positive z-index positioned descendants on top
    let positioned = !matches!(pos, Position::Static);
    let bucket: i32 = if positioned {
        match zi_opt {
            Some(v) if v < 0 => -2,
            Some(v) if v > 0 => 2,
            Some(_) | None => 1,
        }
    } else {
        0
    };
    // Sort by bucket first, then DOM order within each bucket.
    (bucket, child.0)
}

pub trait DisplayBuilder: Send + Sync {
    fn build_retained(&self, inputs: RetainedInputs) -> DisplayList;
    fn build_rect_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
    ) -> Vec<wgpu_renderer::DrawRect>;
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
        computed_map: &HashMap<NodeKey, ComputedStyle>,
    ) -> Vec<wgpu_renderer::DrawText>;
}

pub struct DefaultDisplayBuilder;

impl DisplayBuilder for DefaultDisplayBuilder {
    fn build_retained(&self, inputs: RetainedInputs) -> DisplayList {
        build_retained(inputs)
    }
    fn build_rect_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
    ) -> Vec<wgpu_renderer::DrawRect> {
        build_rect_list(rects, snapshot)
    }
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, LayoutRect>,
        snapshot: SnapshotSlice,
        computed_map: &HashMap<NodeKey, ComputedStyle>,
    ) -> Vec<wgpu_renderer::DrawText> {
        build_text_list(rects, snapshot, computed_map)
    }
}

pub fn build_rect_list(
    rects: &HashMap<NodeKey, LayoutRect>,
    snapshot: SnapshotSlice,
) -> Vec<wgpu_renderer::DrawRect> {
    let mut list: Vec<wgpu_renderer::DrawRect> = Vec::new();
    for (node, kind, _children) in snapshot.iter() {
        if !matches!(kind, LayoutNodeKind::Block { .. }) {
            continue;
        }
        if let Some(rect) = rects.get(node) {
            list.push(wgpu_renderer::DrawRect {
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

pub fn build_text_list(
    rects: &HashMap<NodeKey, LayoutRect>,
    snapshot: SnapshotSlice,
    computed_map: &HashMap<NodeKey, ComputedStyle>,
) -> Vec<wgpu_renderer::DrawText> {
    let mut list: Vec<wgpu_renderer::DrawText> = Vec::new();
    // Build a parent map so inline text can inherit from its element parent
    let mut parent_of: HashMap<NodeKey, NodeKey> = HashMap::new();
    for (parent, _kind, children) in snapshot.iter() {
        for &c in children {
            parent_of.insert(c, *parent);
        }
    }
    // Helper: climb ancestors until a computed style is found
    let nearest_style = |start: NodeKey| -> Option<&ComputedStyle> {
        let mut cur = Some(start);
        while let Some(n) = cur {
            if let Some(cs) = computed_map.get(&n) {
                return Some(cs);
            }
            cur = parent_of.get(&n).copied();
        }
        None
    };
    // Helper: climb ancestors until a rect is found (inline text nodes don't have rects).
    let nearest_rect = |start: NodeKey| -> Option<LayoutRect> {
        let mut cur = Some(start);
        while let Some(n) = cur {
            if let Some(r) = rects.get(&n) {
                return Some(*r);
            }
            cur = parent_of.get(&n).copied();
        }
        None
    };
    for (key, kind, _children) in snapshot.iter() {
        if let LayoutNodeKind::InlineText { text } = kind {
            if text.trim().is_empty() {
                continue;
            }
            let rect = nearest_rect(*key)
                .or_else(|| nearest_rect(parent_of.get(key).copied().unwrap_or(*key)));
            if let Some(rect) = rect {
                let style_opt = nearest_style(*key);
                let (font_size, color_rgb) = if let Some(cs) = style_opt {
                    let c = cs.color;
                    (
                        cs.font_size,
                        [
                            c.red as f32 / 255.0,
                            c.green as f32 / 255.0,
                            c.blue as f32 / 255.0,
                        ],
                    )
                } else {
                    (16.0, [0.0, 0.0, 0.0])
                };
                let collapsed = collapse_whitespace(text);
                if collapsed.is_empty() {
                    continue;
                }
                // When overflow is hidden/clip, clip to the content-box width per CSS Display/Overflow.
                // Border-box rect is provided; compute content-box left and width.
                let (content_left_x, content_width_px) = style_opt
                    .filter(|cs| matches!(cs.overflow, Overflow::Hidden))
                    .map(|cs| {
                        let pad_left = cs.padding.left.max(0.0) as i32;
                        let pad_right = cs.padding.right.max(0.0) as i32;
                        let border_left = cs.border_width.left.max(0.0) as i32;
                        let border_right = cs.border_width.right.max(0.0) as i32;
                        let left_x = (rect.x.round() as i32) + border_left + pad_left;
                        let width_px = (rect.width.round() as i32)
                            .saturating_sub(border_left + pad_left + pad_right + border_right);
                        (left_x, width_px)
                    })
                    .unwrap_or(((rect.x.round() as i32), (rect.width.round() as i32)));
                let max_width_px = content_width_px.max(0);
                // Prefer computed line-height when available; otherwise use real metrics if available.
                let (asc_px, desc_px, lead_px, lh_from_glyph) =
                    derive_line_metrics_from_content(&collapsed, font_size);
                let computed_lh = style_opt.and_then(|cs| cs.line_height);
                let used_line_height = computed_lh.or(lh_from_glyph).unwrap_or_else(|| {
                    // Spec-like normal using metrics; keep a minimum padding to avoid clip.
                    let sum = asc_px + desc_px + lead_px;
                    sum.max(font_size + 2.0)
                });
                let line_height = used_line_height.round() as i32;
                let ascent = asc_px.round() as i32; // placeholder until face metrics are available
                let _descent = desc_px.round() as i32;
                let lines = wrap_text_uax14(&collapsed, font_size, max_width_px);
                for (line_index, raw_line) in lines.iter().enumerate() {
                    let visual_line = reorder_bidi_for_display(raw_line);
                    let line_top = (rect.y.round() as i32) + (line_index as i32) * line_height;
                    let baseline_y = line_top + ascent;
                    // Use line box bounds: top at line_top; bottom at line_top + line_height
                    let top = line_top;
                    let bottom = line_top + line_height;
                    let bounds = Some((content_left_x, top, content_left_x + max_width_px, bottom));
                    list.push(wgpu_renderer::DrawText {
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
    let (asc_px, desc_px, lead_px, lh_from_glyph) =
        derive_line_metrics_from_content(&collapsed, font_size);
    let used_line_height =
        lh_from_glyph.unwrap_or_else(|| (asc_px + desc_px + lead_px).max(font_size + 2.0));
    let line_height = used_line_height.round() as i32;
    let ascent = asc_px.round() as i32; // placeholder until face metrics are available
    let _descent = desc_px.round() as i32;
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

// Break lines using Unicode line breaking (UAX#14), greedily packing runs while
// measuring shaped widths. This improves fidelity for scripts where whitespace-only
// breaking is insufficient.
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
    for (idx, opp) in linebreaks(text) {
        let is_break = matches!(opp, BreakOpportunity::Mandatory | BreakOpportunity::Allowed);
        if is_break {
            // Measure candidate slice
            let candidate = &text[start..idx];
            let w = measure_text_width_px(candidate, font_size);
            if w <= max_width_px {
                last_good = idx;
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
                let end = idx.max(start + 1);
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

pub fn build_retained(inputs: RetainedInputs) -> DisplayList {
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
    for (key, kind, children) in snapshot.into_iter() {
        kind_map.insert(key, kind);
        children_map.insert(key, children);
    }
    // Parent map for inheritance fallbacks
    let mut parent_map: HashMap<NodeKey, NodeKey> = HashMap::new();
    for (parent, children) in children_map.iter() {
        for &c in children {
            parent_map.insert(c, *parent);
        }
    }

    // Helper: nearest ancestor with a computed style (for inline text, etc.)
    fn nearest_style<'a>(
        start: NodeKey,
        parent_map: &HashMap<NodeKey, NodeKey>,
        computed_map: &'a HashMap<NodeKey, ComputedStyle>,
        computed_fallback: &'a HashMap<NodeKey, ComputedStyle>,
        computed_robust: &'a Option<HashMap<NodeKey, ComputedStyle>>,
    ) -> Option<&'a ComputedStyle> {
        let mut cur = Some(start);
        while let Some(n) = cur {
            if let Some(cs) = computed_robust
                .as_ref()
                .and_then(|m| m.get(&n))
                .or_else(|| computed_fallback.get(&n))
                .or_else(|| computed_map.get(&n))
            {
                return Some(cs);
            }
            cur = parent_map.get(&n).copied();
        }
        None
    }
    #[inline]
    fn nearest_rect(
        start: NodeKey,
        parent_map: &HashMap<NodeKey, NodeKey>,
        rects: &HashMap<NodeKey, LayoutRect>,
    ) -> Option<LayoutRect> {
        let mut cur = Some(start);
        while let Some(n) = cur {
            if let Some(r) = rects.get(&n) {
                return Some(*r);
            }
            cur = parent_map.get(&n).copied();
        }
        None
    }

    // Structured walker to reduce argument counts and nesting.
    #[inline]
    fn order_children(
        children: &[NodeKey],
        parent_map: &HashMap<NodeKey, NodeKey>,
        computed_map: &HashMap<NodeKey, ComputedStyle>,
        computed_fallback: &HashMap<NodeKey, ComputedStyle>,
        computed_robust: &Option<HashMap<NodeKey, ComputedStyle>>,
    ) -> Vec<NodeKey> {
        let mut ordered: Vec<NodeKey> = children.to_vec();
        ordered.sort_by_key(|c| {
            let (bucket, dom) = z_key_for_child(
                *c,
                parent_map,
                computed_map,
                computed_fallback,
                computed_robust,
            );
            (bucket, dom)
        });
        ordered
    }

    #[inline]
    fn process_children(list: &mut DisplayList, node: NodeKey, ctx: &WalkCtx<'_>) {
        if let Some(children) = ctx.children_map.get(&node) {
            let ordered = order_children(
                children,
                ctx.parent_map,
                ctx.computed_map,
                ctx.computed_fallback,
                ctx.computed_robust,
            );
            for child in ordered.into_iter() {
                recurse(list, child, ctx);
            }
        }
    }

    fn recurse(list: &mut DisplayList, node: NodeKey, ctx: &WalkCtx<'_>) {
        let kind = match ctx.kind_map.get(&node) {
            Some(k) => k,
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
                let cs_opt = ctx
                    .computed_robust
                    .as_ref()
                    .and_then(|m| m.get(&node))
                    .or_else(|| ctx.computed_fallback.get(&node))
                    .or_else(|| ctx.computed_map.get(&node));
                if let Some(rect) = rect_opt {
                    // Determine if this node establishes a stacking context.
                    // Preference order: Opacity < 1.0, otherwise positioned with non-auto z-index.
                    let style_for_node_ctx = cs_opt;
                    let mut opened_ctx = false;
                    let boundary_opt = style_for_node_ctx.and_then(stacking_boundary_for);
                    if let Some(boundary) = boundary_opt {
                        list.push(DisplayItem::BeginStackingContext { boundary });
                        opened_ctx = true;
                    }
                    // Background fill from computed styles; only paint if non-transparent
                    let fill_rgba_opt = cs_opt.map(|cs| {
                        let bg = cs.background_color;
                        [
                            bg.red as f32 / 255.0,
                            bg.green as f32 / 255.0,
                            bg.blue as f32 / 255.0,
                            bg.alpha as f32 / 255.0,
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
                    if let Some(cs) = cs_opt {
                        push_border_items(list, rect, cs);
                    }
                    // overflow clip
                    let style_for_node = cs_opt.or_else(|| {
                        nearest_style(
                            node,
                            ctx.parent_map,
                            ctx.computed_map,
                            ctx.computed_fallback,
                            ctx.computed_robust,
                        )
                    });
                    let mut opened_clip = false;
                    if let Some(cs) = style_for_node
                        && matches!(
                            cs.overflow,
                            Overflow::Hidden | Overflow::Clip | Overflow::Auto | Overflow::Scroll
                        )
                    {
                        // Clip at the padding box per CSS Overflow spec. Compute padding-box
                        // from the border-box rect and the border widths.
                        let pad_left = cs.padding.left.max(0.0);
                        let pad_top = cs.padding.top.max(0.0);
                        let pad_right = cs.padding.right.max(0.0);
                        let pad_bottom = cs.padding.bottom.max(0.0);
                        let border_left = cs.border_width.left.max(0.0);
                        let border_top = cs.border_width.top.max(0.0);
                        let border_right = cs.border_width.right.max(0.0);
                        let border_bottom = cs.border_width.bottom.max(0.0);
                        let clip_x = rect.x + border_left + pad_left;
                        let clip_y = rect.y + border_top + pad_top;
                        let clip_w = (rect.width
                            - (border_left + pad_left + pad_right + border_right))
                            .max(0.0);
                        let clip_h = (rect.height
                            - (border_top + pad_top + pad_bottom + border_bottom))
                            .max(0.0);
                        list.push(DisplayItem::BeginClip {
                            x: clip_x,
                            y: clip_y,
                            width: clip_w,
                            height: clip_h,
                        });
                        opened_clip = true;
                    }
                    process_children(list, node, ctx);
                    if opened_ctx {
                        list.push(DisplayItem::EndStackingContext);
                    }
                    if opened_clip {
                        list.push(DisplayItem::EndClip);
                    }
                } else {
                    // No rect for this block: still recurse into children; apply stacking context if present
                    let style_for_node_ctx = cs_opt.or_else(|| {
                        nearest_style(
                            node,
                            ctx.parent_map,
                            ctx.computed_map,
                            ctx.computed_fallback,
                            ctx.computed_robust,
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
                    let (font_size, color_rgb) = if let Some(cs) = nearest_style(
                        node,
                        ctx.parent_map,
                        ctx.computed_map,
                        ctx.computed_fallback,
                        ctx.computed_robust,
                    ) {
                        let c = cs.color;
                        (
                            cs.font_size,
                            [
                                c.red as f32 / 255.0,
                                c.green as f32 / 255.0,
                                c.blue as f32 / 255.0,
                            ],
                        )
                    } else {
                        (16.0, [0.0, 0.0, 0.0])
                    };
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

    if let Some((x0, y0, x1, y1)) = selection_overlay {
        let sel_x = x0.min(x1) as f32;
        let sel_y = y0.min(y1) as f32;
        let sel_w = (x0.max(x1) - sel_x.round() as i32).max(0) as f32;
        let sel_h = (y0.max(y1) - sel_y.round() as i32).max(0) as f32;
        let selection = LayoutRect {
            x: sel_x,
            y: sel_y,
            width: sel_w,
            height: sel_h,
        };
        for (_k, rect) in rects.iter() {
            let ix = rect.x.max(selection.x);
            let iy = rect.y.max(selection.y);
            let ix1 = (rect.x + rect.width).min(selection.x + selection.width);
            let iy1 = (rect.y + rect.height).min(selection.y + selection.height);
            let iw = (ix1 - ix).max(0.0);
            let ih = (iy1 - iy).max(0.0);
            if iw > 0.0 && ih > 0.0 {
                list.push(DisplayItem::Rect {
                    x: ix,
                    y: iy,
                    width: iw,
                    height: ih,
                    color: [0.2, 0.5, 1.0, 0.35],
                });
            }
        }
    }

    if let Some(focused) = focused_node
        && let Some(r) = rects.get(&focused)
    {
        let x = r.x;
        let y = r.y;
        let w = r.width;
        let h = r.height;
        let c = [0.2, 0.4, 1.0, 1.0];
        let t = 2.0_f32;
        list.push(DisplayItem::Rect {
            x,
            y,
            width: w,
            height: t,
            color: c,
        });
        list.push(DisplayItem::Rect {
            x,
            y: y + h - t,
            width: w,
            height: t,
            color: c,
        });
        list.push(DisplayItem::Rect {
            x,
            y,
            width: t,
            height: h,
            color: c,
        });
        list.push(DisplayItem::Rect {
            x: x + w - t,
            y,
            width: t,
            height: h,
            color: c,
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
