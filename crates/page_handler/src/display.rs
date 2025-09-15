use js::NodeKey;
use std::collections::HashMap;
use css::layout_helpers::{collapse_whitespace, reorder_bidi_for_display};
use wgpu_renderer::{DisplayItem, DisplayList};

pub struct RetainedInputs {
    pub rects: HashMap<NodeKey, layouter::LayoutRect>,
    pub snapshot: Vec<(NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>)>,
    pub computed_map: HashMap<NodeKey, style_engine::ComputedStyle>,
    pub computed_fallback: HashMap<NodeKey, style_engine::ComputedStyle>,
    pub computed_robust: Option<HashMap<NodeKey, style_engine::ComputedStyle>>,
    pub selection_overlay: Option<(i32, i32, i32, i32)>,
    pub focused_node: Option<NodeKey>,
    pub hud_enabled: bool,
    pub spillover_deferred: u64,
    pub last_style_restyled_nodes: u64,
}

pub trait DisplayBuilder: Send + Sync {
    fn build_retained(&self, inputs: RetainedInputs) -> DisplayList;
    fn build_rect_list(
        &self,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        snapshot: &[(NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>)],
    ) -> Vec<wgpu_renderer::DrawRect>;
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        snapshot: &[(NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>)],
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
        snapshot: &[(NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>)],
    ) -> Vec<wgpu_renderer::DrawRect> {
        build_rect_list(rects, snapshot)
    }
    fn build_text_list(
        &self,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        snapshot: &[(NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>)],
        computed_map: &HashMap<NodeKey, style_engine::ComputedStyle>,
    ) -> Vec<wgpu_renderer::DrawText> {
        build_text_list(rects, snapshot, computed_map)
    }
}

pub fn build_rect_list(
    rects: &HashMap<NodeKey, layouter::LayoutRect>,
    snapshot: &[(NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>)],
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
    snapshot: &[(NodeKey, layouter::LayoutNodeKind, Vec<NodeKey>)],
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
    for (key, kind, _children) in snapshot.iter() {
        if let layouter::LayoutNodeKind::InlineText { text } = kind {
            if text.trim().is_empty() {
                continue;
            }
            if let Some(rect) = rects.get(key) {
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
                let display_text = reorder_bidi_for_display(&collapsed);
                list.push(wgpu_renderer::DrawText {
                    x: rect.x as f32,
                    y: rect.y as f32,
                    text: display_text,
                    color: color_rgb,
                    font_size,
                });
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
    let display_text = reorder_bidi_for_display(&collapsed);
    list.push(DisplayItem::Text {
        x: rect.x as f32,
        y: rect.y as f32,
        text: display_text,
        color: color_rgb,
        font_size,
    });
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

    fn recurse(
        list: &mut DisplayList,
        node: NodeKey,
        kind_map: &HashMap<NodeKey, layouter::LayoutNodeKind>,
        children_map: &HashMap<NodeKey, Vec<NodeKey>>,
        rects: &HashMap<NodeKey, layouter::LayoutRect>,
        computed_map: &HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_fallback: &HashMap<NodeKey, style_engine::ComputedStyle>,
        computed_robust: &Option<HashMap<NodeKey, style_engine::ComputedStyle>>,
        parent_map: &HashMap<NodeKey, NodeKey>,
    ) {
        let kind = match kind_map.get(&node) {
            Some(k) => k,
            None => return,
        };
        match kind {
            layouter::LayoutNodeKind::Document => {
                if let Some(children) = children_map.get(&node) {
                    for &child in children {
                        recurse(
                            list,
                            child,
                            kind_map,
                            children_map,
                            rects,
                            computed_map,
                            computed_fallback,
                            computed_robust,
                            parent_map,
                        );
                    }
                }
            }
            layouter::LayoutNodeKind::Block { .. } => {
                if let Some(rect) = rects.get(&node) {
                    // Background fill from computed styles (with alpha)
                    let mut bg_rgba: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
                    let cs_opt = computed_robust
                        .as_ref()
                        .and_then(|m| m.get(&node))
                        .or_else(|| computed_fallback.get(&node))
                        .or_else(|| computed_map.get(&node));
                    if let Some(cs) = cs_opt {
                        let bg = cs.background_color;
                        bg_rgba = [
                            bg.red as f32 / 255.0,
                            bg.green as f32 / 255.0,
                            bg.blue as f32 / 255.0,
                            bg.alpha as f32 / 255.0,
                        ];
                    }
                    if bg_rgba[3] > 0.0 {
                        list.push(DisplayItem::Rect {
                            x: rect.x as f32,
                            y: rect.y as f32,
                            width: rect.width as f32,
                            height: rect.height as f32,
                            color: bg_rgba,
                        });
                    }
                    // CSS border rendering (solid only for now)
                    if let Some(cs) = cs_opt {
                        use style_engine::BorderStyle;
                        let bw = cs.border_width;
                        let bs = cs.border_style;
                        let bc = cs.border_color;
                        let col = [
                            bc.red as f32 / 255.0,
                            bc.green as f32 / 255.0,
                            bc.blue as f32 / 255.0,
                            bc.alpha as f32 / 255.0,
                        ];
                        let has_alpha = col[3] > 0.0;
                        if has_alpha && matches!(bs, BorderStyle::Solid) {
                            let x = rect.x as f32;
                            let y = rect.y as f32;
                            let w = rect.width as f32;
                            let h = rect.height as f32;
                            let t = bw.top.max(0.0);
                            let r = bw.right.max(0.0);
                            let b = bw.bottom.max(0.0);
                            let l = bw.left.max(0.0);
                            if t > 0.0 {
                                list.push(DisplayItem::Rect {
                                    x,
                                    y,
                                    width: w,
                                    height: t,
                                    color: col,
                                });
                            }
                            if b > 0.0 {
                                list.push(DisplayItem::Rect {
                                    x,
                                    y: y + h - b,
                                    width: w,
                                    height: b,
                                    color: col,
                                });
                            }
                            if l > 0.0 {
                                list.push(DisplayItem::Rect {
                                    x,
                                    y,
                                    width: l,
                                    height: h,
                                    color: col,
                                });
                            }
                            if r > 0.0 {
                                list.push(DisplayItem::Rect {
                                    x: x + w - r,
                                    y,
                                    width: r,
                                    height: h,
                                    color: col,
                                });
                            }
                        }
                    }
                    // overflow clip (style-driven via nearest element style)
                    let mut opened_clip = false;
                    let style_for_node = nearest_style(
                        node,
                        parent_map,
                        computed_map,
                        computed_fallback,
                        computed_robust,
                    );
                    let overflow_hidden = style_for_node
                        .map(|cs| matches!(cs.overflow, style_engine::Overflow::Hidden))
                        .unwrap_or(false);
                    if overflow_hidden {
                        list.push(DisplayItem::BeginClip {
                            x: rect.x as f32,
                            y: rect.y as f32,
                            width: rect.width as f32,
                            height: rect.height as f32,
                        });
                        opened_clip = true;
                    }
                    if let Some(children) = children_map.get(&node) {
                        // Stacking order: negative z-index below, then normal flow/auto, then positive z-index.
                        let mut ordered: Vec<NodeKey> = children.clone();
                        ordered.sort_by_key(|c| {
                            let style = nearest_style(
                                *c,
                                parent_map,
                                computed_map,
                                computed_fallback,
                                computed_robust,
                            );
                            let (_pos, zi) = style
                                .map(|cs| (cs.position, cs.z_index))
                                .unwrap_or((style_engine::Position::Static, None));
                            let bucket: i32 = match zi {
                                Some(v) if v < 0 => -1,
                                Some(v) if v > 0 => 1,
                                Some(_) | None => 0,
                            };
                            // Fixed/absolute with auto still in normal bucket to retain document order among autos
                            (bucket, c.0)
                        });
                        for child in ordered.into_iter() {
                            recurse(
                                list,
                                child,
                                kind_map,
                                children_map,
                                rects,
                                computed_map,
                                computed_fallback,
                                computed_robust,
                                parent_map,
                            );
                        }
                    }
                    if opened_clip {
                        list.push(DisplayItem::EndClip);
                    }
                }
            }
            layouter::LayoutNodeKind::InlineText { text } => {
                if text.trim().is_empty() {
                    return;
                }
                if let Some(rect) = rects.get(&node) {
                    let (font_size, color_rgb) = if let Some(cs) = nearest_style(
                        node,
                        parent_map,
                        computed_map,
                        computed_fallback,
                        computed_robust,
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
                    push_text_item(list, rect, text, font_size, color_rgb);
                }
            }
        }
    }

    let mut list = DisplayList::new();
    recurse(
        &mut list,
        NodeKey::ROOT,
        &kind_map,
        &children_map,
        &rects,
        &computed_map,
        &computed_fallback,
        &computed_robust,
        &parent_map,
    );

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
        let hud = format!(
            "restyled:{} spill:{}",
            last_style_restyled_nodes, spillover_deferred
        );
        list.push(DisplayItem::Text {
            x: 6.0,
            y: 14.0,
            text: hud,
            color: [0.1, 0.1, 0.1],
            font_size: 12.0,
        });
    }

    list
}
