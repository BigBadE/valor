use crate::snapshots::IRect;
use css::layout_helpers::{collapse_whitespace, reorder_bidi_for_display};
use js::NodeKey;
use std::collections::HashMap;
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
type SnapshotItem = (NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>);
type SnapshotSlice<'a> = &'a [SnapshotItem];

pub struct RetainedInputs {
    pub rects: HashMap<NodeKey, layouter::LayoutRect>,
    pub snapshot: Vec<SnapshotItem>,
    pub computed_map: HashMap<NodeKey, style_engine::ComputedStyle>,
    pub computed_fallback: HashMap<NodeKey, style_engine::ComputedStyle>,
    pub computed_robust: Option<HashMap<NodeKey, style_engine::ComputedStyle>>,
    pub selection_overlay: Option<IRect>,
    pub focused_node: Option<NodeKey>,
    pub hud_enabled: bool,
    pub spillover_deferred: u64,
    pub last_style_restyled_nodes: u64,
}

// Obtain approximate ascent/descent for given font size using glyphon metrics.
// glyphon exposes Metrics { font_size, line_height }. Use font_size as ascent proxy and
// (line_height - ascent) as descent proxy when shaping is not yet providing per-face metrics.
#[inline]
fn glyph_metrics_for_size(font_size: f32) -> (f32, f32) {
    // Until per-face ascent/descent is exposed, use typical ratios.
    // Common Latin fonts: ascent ~0.8, descent ~0.2 of em.
    let ascent = font_size * 0.8;
    let descent = font_size * 0.2;
    (ascent, descent)
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
fn push_border_items(
    list: &mut DisplayList,
    rect: &layouter::LayoutRect,
    cs: &style_engine::ComputedStyle,
) {
    if let Some(items) = build_border_items(rect, cs) {
        for item in items {
            list.push(item);
        }
    }
}

#[inline]
fn build_border_items(
    rect: &layouter::LayoutRect,
    cs: &style_engine::ComputedStyle,
) -> Option<Vec<DisplayItem>> {
    use style_engine::BorderStyle;
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
    let x = rect.x as f32;
    let y = rect.y as f32;
    let w = rect.width as f32;
    let h = rect.height as f32;
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
    computed_map: &HashMap<NodeKey, style_engine::ComputedStyle>,
    computed_fallback: &HashMap<NodeKey, style_engine::ComputedStyle>,
    computed_robust: &Option<HashMap<NodeKey, style_engine::ComputedStyle>>,
) -> (i32, u64) {
    // Inline nearest-style lookup to avoid deep nesting and inner function coupling
    let style = {
        let mut current = Some(child);
        let mut found: Option<&style_engine::ComputedStyle> = None;
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
    let (_pos, zi) = style
        .map(|cs| (cs.position, cs.z_index))
        .unwrap_or((style_engine::Position::Static, None));
    let bucket: i32 = match zi {
        Some(v) if v < 0 => -1,
        Some(v) if v > 0 => 1,
        Some(_) | None => 0,
    };
    (bucket, child.0)
}

pub trait DisplayBuilder: Send + Sync {
    fn build_retained(&self, inputs: RetainedInputs) -> DisplayList;
    fn build_rect_list(
        &self,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        snapshot: SnapshotSlice,
    ) -> Vec<wgpu_renderer::DrawRect>;
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        snapshot: SnapshotSlice,
        computed_map: &HashMap<NodeKey, style_engine::ComputedStyle>,
    ) -> Vec<wgpu_renderer::DrawText>;
}

pub struct DefaultDisplayBuilder;

impl DisplayBuilder for DefaultDisplayBuilder {
    fn build_retained(&self, inputs: RetainedInputs) -> DisplayList {
        build_retained(inputs)
    }
    fn build_rect_list(
        &self,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        snapshot: SnapshotSlice,
    ) -> Vec<wgpu_renderer::DrawRect> {
        build_rect_list(rects, snapshot)
    }
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        snapshot: SnapshotSlice,
        computed_map: &HashMap<NodeKey, style_engine::ComputedStyle>,
    ) -> Vec<wgpu_renderer::DrawText> {
        build_text_list(rects, snapshot, computed_map)
    }
}

pub fn build_rect_list(
    rects: &HashMap<NodeKey, layouter::LayoutRect>,
    snapshot: SnapshotSlice,
) -> Vec<wgpu_renderer::DrawRect> {
    let mut list: Vec<wgpu_renderer::DrawRect> = Vec::new();
    for (node, kind, _children) in snapshot.iter() {
        if !matches!(kind, layouter::LayoutNodeKind::Block { .. }) {
            continue;
        }
        if let Some(rect) = rects.get(node) {
            list.push(wgpu_renderer::DrawRect {
                x: rect.x as f32,
                y: rect.y as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                color: [1.0, 1.0, 1.0],
            });
        }
    }
    list
}

pub fn build_text_list(
    rects: &HashMap<NodeKey, layouter::LayoutRect>,
    snapshot: SnapshotSlice,
    computed_map: &HashMap<NodeKey, style_engine::ComputedStyle>,
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
    let nearest_style = |start: NodeKey| -> Option<&style_engine::ComputedStyle> {
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
    let nearest_rect = |start: NodeKey| -> Option<layouter::LayoutRect> {
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
        if let layouter::LayoutNodeKind::InlineText { text } = kind {
            if text.trim().is_empty() {
                continue;
            }
            let rect = nearest_rect(*key)
                .or_else(|| nearest_rect(parent_of.get(key).copied().unwrap_or(*key)));
            if let Some(rect) = rect {
                let (font_size, color_rgb) = if let Some(cs) = nearest_style(*key) {
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
                let max_width_px = rect.width.max(0);
                // Prefer computed line-height when available; otherwise use 'normal' approximation.
                let computed_lh = nearest_style(*key).and_then(|cs| cs.line_height);
                let line_height = computed_lh
                    .unwrap_or((font_size * 1.2).max(font_size + 2.0))
                    .round() as i32;
                let (asc_px, desc_px) = glyph_metrics_for_size(font_size);
                let ascent = asc_px.round() as i32; // placeholder until face metrics are available
                let _descent = desc_px.round() as i32;
                let lines = wrap_text_uax14(&collapsed, font_size, max_width_px);
                for (line_index, raw_line) in lines.iter().enumerate() {
                    let visual_line = reorder_bidi_for_display(raw_line);
                    let line_top = rect.y + (line_index as i32) * line_height;
                    let baseline_y = line_top + ascent;
                    // Use line box bounds: top at line_top; bottom at line_top + line_height
                    let top = line_top;
                    let bottom = line_top + line_height;
                    let bounds = Some((rect.x, top, rect.x + rect.width, bottom));
                    list.push(wgpu_renderer::DrawText {
                        x: rect.x as f32,
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
    rect: &layouter::LayoutRect,
    text: &str,
    font_size: f32,
    color_rgb: [f32; 3],
) {
    let collapsed = collapse_whitespace(text);
    if collapsed.is_empty() {
        return;
    }
    let max_width_px = rect.width.max(0);
    // Immediate path does not have computed styles; use 'normal' approximation.
    let line_height = ((font_size * 1.2).max(font_size + 2.0)).round() as i32;
    let (asc_px, desc_px) = glyph_metrics_for_size(font_size);
    let ascent = asc_px.round() as i32; // placeholder until face metrics are available
    let _descent = desc_px.round() as i32;
    let broken_lines = wrap_text_uax14(&collapsed, font_size, max_width_px);
    for (line_index, raw_line) in broken_lines.iter().enumerate() {
        let visual_line = reorder_bidi_for_display(raw_line);
        let line_top = rect.y + (line_index as i32) * line_height;
        let baseline_y = line_top + ascent;
        let top = line_top;
        let bottom = line_top + line_height;
        let bounds = Some((rect.x, top, rect.x + rect.width, bottom));
        list.push(DisplayItem::Text {
            x: rect.x as f32,
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

    let mut kind_map: HashMap<NodeKey, layouter::LayoutNodeKind> = HashMap::new();
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
        computed_map: &'a HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_fallback: &'a HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_robust: &'a Option<HashMap<NodeKey, style_engine::ComputedStyle>>,
    ) -> Option<&'a style_engine::ComputedStyle> {
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
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
    ) -> Option<layouter::LayoutRect> {
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
        computed_map: &HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_fallback: &HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_robust: &Option<HashMap<NodeKey, style_engine::ComputedStyle>>,
    ) -> Vec<NodeKey> {
        let mut ordered: Vec<NodeKey> = children.to_vec();
        ordered.sort_by_key(|c| {
            z_key_for_child(
                *c,
                parent_map,
                computed_map,
                computed_fallback,
                computed_robust,
            )
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
    struct WalkCtx<'a> {
        kind_map: &'a HashMap<NodeKey, layouter::LayoutNodeKind>,
        children_map: &'a HashMap<NodeKey, Vec<NodeKey>>,
        rects: &'a HashMap<NodeKey, layouter::LayoutRect>,
        computed_map: &'a HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_fallback: &'a HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_robust: &'a Option<HashMap<NodeKey, style_engine::ComputedStyle>>,
        parent_map: &'a HashMap<NodeKey, NodeKey>,
    }

    fn recurse(list: &mut DisplayList, node: NodeKey, ctx: &WalkCtx<'_>) {
        let kind = match ctx.kind_map.get(&node) {
            Some(k) => k,
            None => return,
        };
        match kind {
            layouter::LayoutNodeKind::Document => {
                if let Some(children) = ctx.children_map.get(&node) {
                    for &child in children {
                        recurse(list, child, ctx);
                    }
                }
            }
            layouter::LayoutNodeKind::Block { .. } => {
                // Background/border/clip only if we have a rect for this node
                let rect_opt = ctx.rects.get(&node);
                let cs_opt = ctx
                    .computed_robust
                    .as_ref()
                    .and_then(|m| m.get(&node))
                    .or_else(|| ctx.computed_fallback.get(&node))
                    .or_else(|| ctx.computed_map.get(&node));
                if let Some(rect) = rect_opt {
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
                            x: rect.x as f32,
                            y: rect.y as f32,
                            width: rect.width as f32,
                            height: rect.height as f32,
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
                        && matches!(cs.overflow, style_engine::Overflow::Hidden)
                    {
                        list.push(DisplayItem::BeginClip {
                            x: rect.x as f32,
                            y: rect.y as f32,
                            width: rect.width as f32,
                            height: rect.height as f32,
                        });
                        opened_clip = true;
                    }
                    // Always recurse into children, independent of having a rect
                    process_children(list, node, ctx);
                    if opened_clip {
                        list.push(DisplayItem::EndClip);
                    }
                } else {
                    // No rect for this block: still recurse into children
                    process_children(list, node, ctx);
                }
            }
            layouter::LayoutNodeKind::InlineText { text } => {
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

    #[cfg(debug_assertions)]
    {
        eprintln!(
            "[DL DEBUG] build_retained produced items={}",
            list.items.len()
        );
    }

    if let Some((x0, y0, x1, y1)) = selection_overlay {
        let sel_x = x0.min(x1);
        let sel_y = y0.min(y1);
        let sel_w = (x0.max(x1) - sel_x).max(0);
        let sel_h = (y0.max(y1) - sel_y).max(0);
        let selection = layouter::LayoutRect {
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
            let iw = (ix1 - ix).max(0);
            let ih = (iy1 - iy).max(0);
            if iw > 0 && ih > 0 {
                list.push(DisplayItem::Rect {
                    x: ix as f32,
                    y: iy as f32,
                    width: iw as f32,
                    height: ih as f32,
                    color: [0.2, 0.5, 1.0, 0.35],
                });
            }
        }
    }

    if let Some(focused) = focused_node
        && let Some(r) = rects.get(&focused)
    {
        let x = r.x as f32;
        let y = r.y as f32;
        let w = r.width as f32;
        let h = r.height as f32;
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
