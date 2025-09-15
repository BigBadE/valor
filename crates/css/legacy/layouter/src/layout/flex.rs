//! Flex layout (row, nowrap): Phase 3 subset with sizing and basic alignment.
//!
//! Implements a small, spec-aligned subset sufficient for simple pages:
//! - Main axis: row; no wrapping.
//! - Sizing: flex-basis, flex-grow, flex-shrink with min/max constraints.
//! - Cross-axis alignment: align-items (flex-start, center, stretch baseline→start).

use std::collections::HashMap;
use js::NodeKey;
use style_engine::{SizeSpecified, AlignItems, JustifyContent, FlexWrap};

use crate::LayoutNodeKind;
use super::args::{ComputeGeomArgs, LayoutMaps};
use super::geometry::LayoutRect;
use super::block::measure_descendant_inline_text_width;

/// Lay out children of a flex container in a horizontal row without wrapping.
/// Returns (content_height, consumed_height).
pub(crate) fn layout_flex_children(
    container: NodeKey,
    maps: &LayoutMaps,
    rects: &mut HashMap<NodeKey, LayoutRect>,
    args: ComputeGeomArgs,
    x_start: i32,
    y_start: i32,
    content_width: i32,
    container_cross_px: Option<i32>,
) -> (i32, i32) {
    let mut max_cross_size: i32 = 0;

    let parent_font_size: f32 = if let Some(cm) = maps.computed_by_key {
        cm.get(&container).map(|cs| cs.font_size).unwrap_or(16.0)
    } else { 16.0 };
    let align_items = if let Some(cm) = maps.computed_by_key {
        cm.get(&container).map(|cs| cs.align_items).unwrap_or(AlignItems::Stretch)
    } else { AlignItems::Stretch };
    let justify_content = if let Some(cm) = maps.computed_by_key {
        cm.get(&container).map(|cs| cs.justify_content).unwrap_or(JustifyContent::FlexStart)
    } else { JustifyContent::FlexStart };
    let flex_wrap = if let Some(cm) = maps.computed_by_key {
        cm.get(&container).map(|cs| cs.flex_wrap).unwrap_or(FlexWrap::NoWrap)
    } else { FlexWrap::NoWrap };

    let children = match maps.children_by_key.get(&container) { Some(v) => v.clone(), None => Vec::new() };
    if children.is_empty() { return (0, 0); }

    // Step 1: Establish flex base sizes and flex factors, including min/max width constraints.
    #[derive(Clone, Debug)]
    struct Item {
        key: NodeKey,
        base: i32,
        min: i32,
        max: i32,
        grow: f32,
        shrink: f32,
        height: i32,
        margin_l: i32,
        margin_r: i32,
        margin_t: i32,
        margin_b: i32,
    }
    let mut items: Vec<Item> = Vec::with_capacity(children.len());

    for child in &children {
        // Skip whitespace-only text nodes; they should not create flex items
        if let Some(LayoutNodeKind::InlineText { text }) = maps.kind_by_key.get(child)
            && text.trim().is_empty()
        { continue; }
        // Fetch computed style if available
        let (width_spec, height_spec, flex_basis, flex_grow, flex_shrink, min_w, max_w, child_font_size, margin_l, margin_r, margin_t, margin_b) = if let Some(cm) = maps.computed_by_key {
            if let Some(cs) = cm.get(child) {
                (
                    cs.width,
                    cs.height,
                    cs.flex_basis,
                    cs.flex_grow,
                    cs.flex_shrink,
                    cs.min_width,
                    cs.max_width,
                    cs.font_size,
                    cs.margin.left.round() as i32,
                    cs.margin.right.round() as i32,
                    cs.margin.top.round() as i32,
                    cs.margin.bottom.round() as i32,
                )
            } else { (SizeSpecified::Auto, SizeSpecified::Auto, SizeSpecified::Auto, 0.0, 1.0, None, None, parent_font_size, 0, 0, 0, 0) }
        } else { (SizeSpecified::Auto, SizeSpecified::Auto, SizeSpecified::Auto, 0.0, 1.0, None, None, parent_font_size, 0, 0, 0, 0) };

        // Base size: flex-basis, else width, else content-based estimate
        let mut base: i32 = match flex_basis {
            SizeSpecified::Px(px) => px.round() as i32,
            SizeSpecified::Percent(p) => (p * content_width as f32).round() as i32,
            SizeSpecified::Auto => match width_spec {
                SizeSpecified::Px(px) => px.round() as i32,
                SizeSpecified::Percent(p) => (p * content_width as f32).round() as i32,
                SizeSpecified::Auto => {
                    // Content size approximation
                    match maps.kind_by_key.get(child) {
                        Some(LayoutNodeKind::InlineText { text }) => {
                            let scale = child_font_size / 16.0;
                            ((text.chars().count() as f32 * args.char_width as f32) * scale).round() as i32
                        }
                        _ => measure_descendant_inline_text_width(*child, maps, args.char_width, child_font_size),
                    }
                }
            }
        };

        // Cross size (height): specified height in px or from line-height
        let mut height: i32 = match height_spec { SizeSpecified::Px(px) => px.round() as i32, _ => 0 };
        if height == 0 {
            let lh = if let Some(cm) = maps.computed_by_key { cm.get(child).map(|cs| (cs.line_height * cs.font_size).round() as i32).unwrap_or((1.2 * child_font_size).round() as i32) } else { (1.2 * child_font_size).round() as i32 };
            height = lh.max(1);
        }
        max_cross_size = max_cross_size.max(height);

        // Min/max constraints
        let clamp_size = |spec: &Option<SizeSpecified>| -> Option<i32> {
            match spec {
                Some(SizeSpecified::Px(px)) => Some(px.round() as i32),
                Some(SizeSpecified::Percent(p)) => Some((p * content_width as f32).round() as i32),
                Some(SizeSpecified::Auto) => None,
                None => None,
            }
        };
        let min_px = clamp_size(&min_w).unwrap_or(0);
        let max_px = clamp_size(&max_w).unwrap_or(i32::MAX);
        base = base.clamp(min_px, max_px);

        items.push(Item { key: *child, base, min: min_px, max: max_px, grow: flex_grow.max(0.0), shrink: flex_shrink.max(0.0), height, margin_l, margin_r, margin_t, margin_b });
    }

    // Step 2: Prepare helpers for distributing free space using grow/shrink.
    let mut integer_sizes: Vec<i32> = items.iter().map(|it| it.base).collect();

    // Helper: finalize float sizes to integers, distributing rounding remainder to the first items.
    let finalize_sizes = |float_sizes: &Vec<f32>, constraints: &Vec<(i32, i32)>, must_fit: bool, target_main: i32| -> Vec<i32> {
        // Start with floors to compute clean fractional remainders
        let mut floors: Vec<i32> = float_sizes.iter().map(|v| v.floor() as i32).collect();
        // If we don't need to fit to container (e.g., grow=0 and positive free space), just clamp and return.
        if !must_fit {
            for (idx, (min_px, max_px)) in constraints.iter().enumerate() {
                floors[idx] = floors[idx].clamp(*min_px, *max_px);
            }
            return floors;
        }
        // Adjust rounding to exactly match container width if needed.
        let sum_after_floor: i32 = floors.iter().sum();
        let mut diff: i32 = target_main - sum_after_floor;
        if diff == 0 {
            for (idx, (min_px, max_px)) in constraints.iter().enumerate() {
                floors[idx] = floors[idx].clamp(*min_px, *max_px);
            }
            return floors;
        }
        // Build ordering by descending fractional part, tie-breaking left-to-right
        let mut order: Vec<usize> = (0..float_sizes.len()).collect();
        order.sort_by(|&a, &b| {
            let fa = float_sizes[a] - (floors[a] as f32);
            let fb = float_sizes[b] - (floors[b] as f32);
            fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(&b))
        });
        let mut out = floors;
        if diff > 0 {
            let mut remaining = diff;
            for &idx in &order {
                if remaining == 0 { break; }
                let (_min_px, max_px) = constraints[idx];
                if out[idx] < max_px {
                    out[idx] += 1;
                    remaining -= 1;
                }
            }
        } else {
            let mut remaining = -diff;
            for &idx in &order {
                if remaining == 0 { break; }
                let (min_px, _max_px) = constraints[idx];
                if out[idx] > min_px {
                    out[idx] -= 1;
                    remaining -= 1;
                }
            }
        }
        for (idx, (min_px, max_px)) in constraints.iter().enumerate() {
            out[idx] = out[idx].clamp(*min_px, *max_px);
        }
        out
    };

    // Build lines using base sizes and margins (no distribution yet)
    // Step 3: Build lines (wrap or single-line) with positions.
    #[derive(Default)]
    struct Line { indices: Vec<usize>, total_main: i32, max_cross: i32 }
    let mut lines: Vec<Line> = Vec::new();
    let mut cur = Line::default();
    for (idx, it) in items.iter().enumerate() {
        let main_with_margins = integer_sizes[idx] + it.margin_l + it.margin_r;
        if flex_wrap == FlexWrap::Wrap && !cur.indices.is_empty() && cur.total_main + main_with_margins > content_width {
            lines.push(cur);
            cur = Line::default();
        }
        cur.total_main += main_with_margins;
        cur.max_cross = cur.max_cross.max(it.height + it.margin_t + it.margin_b);
        cur.indices.push(idx);
    }
    if !cur.indices.is_empty() { lines.push(cur); }

    // Step 2b: Distribute free space per line when wrapping, or for all items when no-wrap
    if flex_wrap == FlexWrap::NoWrap {
        // Compute available main excluding margins across all items
        let total_margins: i32 = items.iter().map(|it| it.margin_l + it.margin_r).sum();
        let available_main: i32 = (content_width - total_margins).max(0);
        let total_base: i32 = items.iter().map(|it| it.base).sum();
        let free_space: i32 = available_main - total_base;
        let total_grow: f32 = items.iter().map(|it| it.grow).sum();
        let total_shrink_factor: f32 = items.iter().map(|it| it.shrink * (it.base as f32)).sum();
        if free_space == 0 || (free_space > 0 && total_grow == 0.0) || (free_space < 0 && total_shrink_factor == 0.0) {
            for (idx, it) in items.iter().enumerate() { integer_sizes[idx] = integer_sizes[idx].clamp(it.min, it.max); }
        } else {
            let mut float_sizes: Vec<f32> = items.iter().map(|it| it.base as f32).collect();
            let mut frozen: Vec<bool> = vec![false; items.len()];
            let mut iterations = 0;
            loop {
                iterations += 1; if iterations > 8 { break; }
                let current_sum: f32 = float_sizes.iter().sum();
                let remaining: f32 = (available_main as f32) - current_sum;
                if remaining.abs() < 0.5 { break; }
                let mut total_weight: f32 = 0.0;
                for (i, it) in items.iter().enumerate() {
                    if frozen[i] { continue; }
                    let w = if free_space > 0 { it.grow } else { it.shrink * (it.base as f32) };
                    if w > 0.0 { total_weight += w; }
                }
                if total_weight <= 0.0 { break; }
                let mut any_newly_frozen = false;
                for (i, it) in items.iter().enumerate() {
                    if frozen[i] { continue; }
                    let weight = if free_space > 0 { it.grow } else { it.shrink * (it.base as f32) };
                    if weight <= 0.0 { continue; }
                    let share = remaining * (weight / total_weight);
                    let proposed = float_sizes[i] + share;
                    let clamped = proposed.clamp(it.min as f32, it.max as f32);
                    float_sizes[i] = clamped;
                    if (clamped - proposed).abs() > 0.0001 { frozen[i] = true; any_newly_frozen = true; }
                }
                if !any_newly_frozen { break; }
            }
            let constraints: Vec<(i32, i32)> = items.iter().map(|it| (it.min, it.max)).collect();
            integer_sizes = finalize_sizes(&float_sizes, &constraints, true, available_main);
        }
    } else {
        // Wrap: distribute per line
        for line in &lines {
            let total_margins: i32 = line.indices.iter().map(|&i| items[i].margin_l + items[i].margin_r).sum();
            let available_main: i32 = (content_width - total_margins).max(0);
            let total_base: i32 = line.indices.iter().map(|&i| items[i].base).sum();
            let free_space: i32 = available_main - total_base;
            let total_grow: f32 = line.indices.iter().map(|&i| items[i].grow).sum();
            let total_shrink_factor: f32 = line.indices.iter().map(|&i| items[i].shrink * (items[i].base as f32)).sum();
            if free_space == 0 || (free_space > 0 && total_grow == 0.0) || (free_space < 0 && total_shrink_factor == 0.0) {
                for &idx in &line.indices { integer_sizes[idx] = integer_sizes[idx].clamp(items[idx].min, items[idx].max); }
            } else {
                let mut float_sizes: Vec<f32> = line.indices.iter().map(|&i| items[i].base as f32).collect();
                let mut frozen: Vec<bool> = vec![false; line.indices.len()];
                let mut iterations = 0;
                loop {
                    iterations += 1; if iterations > 8 { break; }
                    let current_sum: f32 = float_sizes.iter().sum();
                    let remaining: f32 = (available_main as f32) - current_sum;
                    if remaining.abs() < 0.5 { break; }
                    let mut total_weight: f32 = 0.0;
                    for (li, &i) in line.indices.iter().enumerate() {
                        if frozen[li] { continue; }
                        let w = if free_space > 0 { items[i].grow } else { items[i].shrink * (items[i].base as f32) };
                        if w > 0.0 { total_weight += w; }
                    }
                    if total_weight <= 0.0 { break; }
                    let mut any_newly_frozen = false;
                    for (li, &i) in line.indices.iter().enumerate() {
                        if frozen[li] { continue; }
                        let weight = if free_space > 0 { items[i].grow } else { items[i].shrink * (items[i].base as f32) };
                        if weight <= 0.0 { continue; }
                        let share = remaining * (weight / total_weight);
                        let proposed = float_sizes[li] + share;
                        let clamped = proposed.clamp(items[i].min as f32, items[i].max as f32);
                        float_sizes[li] = clamped;
                        if (clamped - proposed).abs() > 0.0001 { frozen[li] = true; any_newly_frozen = true; }
                    }
                    if !any_newly_frozen { break; }
                }
                let constraints: Vec<(i32, i32)> = line.indices.iter().map(|&i| (items[i].min, items[i].max)).collect();
                let finalized = finalize_sizes(&float_sizes, &constraints, true, available_main);
                for (li, &i) in line.indices.iter().enumerate() { integer_sizes[i] = finalized[li]; }
            }
        }
    }

    let sizes = integer_sizes;

    // Step 4: Position lines and items with justify-content and align-items.
    let mut y_line = y_start;
    // align-content handling (default: stretch): if the container has a definite height and wraps,
    // distribute extra cross space equally among lines.
    let mut per_line_extra: i32 = 0;
    if flex_wrap == FlexWrap::Wrap {
        let container_cross = container_cross_px;
        if let Some(h) = container_cross {
            let sum_cross: i32 = lines.iter().map(|ln| ln.max_cross).sum();
            if h > sum_cross && !lines.is_empty() {
                per_line_extra = (h - sum_cross) / (lines.len() as i32);
            }
        }
    }
    for line in &lines {
        let effective_line_cross = line.max_cross + per_line_extra;
        let free_space = (content_width - line.total_main).max(0);
        let gap_between = if line.indices.len() >= 2 {
            match justify_content {
                JustifyContent::SpaceBetween => free_space / ((line.indices.len() - 1) as i32),
                _ => 0,
            }
        } else { 0 };
        let initial_offset = match justify_content {
            JustifyContent::Center => free_space / 2,
            JustifyContent::FlexEnd => free_space,
            _ => 0,
        };
        let mut x = x_start + initial_offset;
        for (pos, &idx) in line.indices.iter().enumerate() {
            let it = &items[idx];
            let width = sizes[idx].max(0);
            let height = it.height.max(0);
            let occupied = height + it.margin_t + it.margin_b;
            let y = match align_items {
                AlignItems::Center => y_line + ((effective_line_cross - occupied) / 2).max(0) + it.margin_t,
                AlignItems::FlexEnd => y_line + (effective_line_cross - occupied).max(0) + it.margin_t,
                AlignItems::FlexStart | AlignItems::Baseline | AlignItems::Stretch => y_line + it.margin_t,
            };
            x += it.margin_l;
            rects.insert(it.key, LayoutRect { x, y, width, height });
            x += width + it.margin_r;
            if gap_between > 0 && pos + 1 < line.indices.len() { x += gap_between; }
        }
        y_line += effective_line_cross;
        max_cross_size = max_cross_size.max(effective_line_cross);
    }

    let content_height = y_line - y_start;
    (content_height, content_height)
}
