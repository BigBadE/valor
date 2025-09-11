//! Flex layout (row, nowrap): Phase 3 subset with sizing and basic alignment.
//!
//! Implements a small, spec-aligned subset sufficient for simple pages:
//! - Main axis: row; no wrapping.
//! - Sizing: flex-basis, flex-grow, flex-shrink with min/max constraints.
//! - Cross-axis alignment: align-items (flex-start, center, stretch baseline→start).

use std::collections::HashMap;
use js::NodeKey;
use style_engine::{SizeSpecified, AlignItems};

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
) -> (i32, i32) {
    let mut max_cross_size: i32 = 0;

    let parent_font_size: f32 = if let Some(cm) = maps.computed_by_key {
        cm.get(&container).map(|cs| cs.font_size).unwrap_or(16.0)
    } else { 16.0 };
    let align_items = if let Some(cm) = maps.computed_by_key {
        cm.get(&container).map(|cs| cs.align_items).unwrap_or(AlignItems::Stretch)
    } else { AlignItems::Stretch };

    let children = match maps.children_by_key.get(&container) { Some(v) => v.clone(), None => Vec::new() };
    if children.is_empty() { return (0, 0); }

    // Step 1: Establish flex base sizes and flex factors, including min/max width constraints.
    #[derive(Clone, Debug)]
    struct Item { key: NodeKey, base: i32, min: i32, max: i32, grow: f32, shrink: f32, height: i32 }
    let mut items: Vec<Item> = Vec::with_capacity(children.len());

    for child in &children {
        // Skip whitespace-only text nodes; they should not create flex items
        if let Some(LayoutNodeKind::InlineText { text }) = maps.kind_by_key.get(child) {
            if text.trim().is_empty() { continue; }
        }
        // Fetch computed style if available
        let (width_spec, height_spec, flex_basis, flex_grow, flex_shrink, min_w, max_w, child_font_size) = if let Some(cm) = maps.computed_by_key {
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
                )
            } else { (SizeSpecified::Auto, SizeSpecified::Auto, SizeSpecified::Auto, 0.0, 1.0, None, None, parent_font_size) }
        } else { (SizeSpecified::Auto, SizeSpecified::Auto, SizeSpecified::Auto, 0.0, 1.0, None, None, parent_font_size) };

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

        items.push(Item { key: *child, base, min: min_px, max: max_px, grow: flex_grow.max(0.0), shrink: flex_shrink.max(0.0), height });
    }

    // Step 2: Distribute free space using grow/shrink.
    let total_base: i32 = items.iter().map(|it| it.base).sum();
    let mut sizes: Vec<i32> = items.iter().map(|it| it.base).collect();
    let mut free_space: i32 = content_width - total_base;

    if free_space > 0 {
        let total_grow: f32 = items.iter().map(|it| it.grow).sum();
        if total_grow > 0.0 {
            let mut remaining = free_space as f32;
            for (idx, it) in items.iter().enumerate() {
                let share = (free_space as f32) * (it.grow / total_grow);
                let delta = share.round();
                sizes[idx] = (sizes[idx] as f32 + delta).round() as i32;
                remaining -= delta;
            }
            // If rounding left pixels, add to last item
            if remaining.abs() >= 1.0 {
                if let Some(last) = sizes.last_mut() { *last += remaining.round() as i32; }
            }
        }
    } else if free_space < 0 {
        let total_shrink_factor: f32 = items.iter().map(|it| it.shrink * (it.base as f32)).sum();
        if total_shrink_factor > 0.0 {
            let deficit = (-free_space) as f32;
            let mut distributed = 0.0f32;
            for (idx, it) in items.iter().enumerate() {
                let factor = it.shrink * (it.base as f32);
                let share = deficit * (factor / total_shrink_factor);
                let delta = share.round();
                sizes[idx] = (sizes[idx] as f32 - delta).round() as i32;
                distributed += delta;
            }
            // Clamp to min sizes and adjust if necessary
            for (idx, it) in items.iter().enumerate() {
                if sizes[idx] < it.min { sizes[idx] = it.min; }
            }
            // Ensure we don't exceed container due to clamping; soft adjustment on last item
            let total_after: i32 = sizes.iter().sum();
            let overflow = total_after - content_width;
            if overflow > 0 { if let Some(last) = sizes.last_mut() { *last = (*last - overflow).max(items.last().map(|it| it.min).unwrap_or(0)); } }
        }
    }

    // Enforce max constraints after distribution
    for (idx, it) in items.iter().enumerate() { sizes[idx] = sizes[idx].clamp(it.min, it.max); }
    // After rounding and clamping, the sum can drift from the container width by ±1.
    // Correct by adjusting the last item's width within its constraints.
    let sum_after: i32 = sizes.iter().sum();
    if sum_after != content_width {
        let diff: i32 = content_width - sum_after;
        if let Some((last_size, last_item)) = sizes.last_mut().zip(items.last()) {
            let new_last = (*last_size + diff).clamp(last_item.min, last_item.max);
            *last_size = new_last;
        }
    }

    // Step 3: Position items along x and align along y.
    let mut x = x_start;
    for (idx, it) in items.iter().enumerate() {
        let width = sizes[idx].max(0);
        let height = it.height.max(0);
        let y = match align_items {
            AlignItems::Center => y_start + ((max_cross_size - height) / 2).max(0),
            AlignItems::FlexStart | AlignItems::Baseline | AlignItems::Stretch | AlignItems::FlexEnd => {
                if matches!(align_items, AlignItems::FlexEnd) { y_start + (max_cross_size - height).max(0) } else { y_start }
            }
        };
        rects.insert(it.key, LayoutRect { x, y, width, height });
        x += width;
    }

    let content_height = max_cross_size;
    (content_height, content_height)
}
