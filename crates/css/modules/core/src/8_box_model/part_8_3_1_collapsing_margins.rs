//! Spec: CSS 2.2 §8.3.1 Collapsing margins
//!
//! This file re-exports the core collapsing margin algorithms from their
//! current implementation locations to provide a spec-mirrored module path.

use css_box::{BoxSides, compute_box_sides};
use css_display::normalize_children;
use css_orchestrator::style_model::{Clear, ComputedStyle};
use js::NodeKey;
use log::debug;

use crate::chapter9::part_9_4_1_block_formatting_context::establishes_block_formatting_context;
use crate::{ChildLayoutCtx, ContainerMetrics, LayoutNodeKind, Layouter};

// Convenience pub fns that forward to inherent methods for mapping granularity.
// Keep functions small and shallow to follow coding standards.

/// Spec: §8.3.1 — Collapse two vertical margins (pair rules).
#[allow(
    dead_code,
    reason = "Public API kept for direct spec-mapped calls; used by tests/future orchestrator paths"
)]
#[inline]
pub fn collapse_margins_pair(left: i32, right: i32) -> i32 {
    if left >= 0i32 && right >= 0i32 {
        return left.max(right);
    }
    if left <= 0i32 && right <= 0i32 {
        return left.min(right);
    }
    left.saturating_add(right)
}

/// Spec: §8.3.1 — Compute the vertical offset from collapsed margins above a block child.
#[inline]
pub fn compute_collapsed_vertical_margin_public(
    ctx: &ChildLayoutCtx,
    child_margin_top: i32,
    _child_style: &ComputedStyle,
) -> i32 {
    if ctx.is_first_placed {
        if ctx.parent_edge_collapsible {
            // Collapse with parent's own top margin applied at the edge; return the
            // incremental collapsed amount beyond the parent's own top margin.
            let combined = collapse_margins_pair(ctx.parent_self_top_margin, child_margin_top);
            return combined.saturating_sub(ctx.parent_self_top_margin);
        }
        // Parent edge is not collapsible (e.g., establishes a BFC). Use child's own top margin.
        return child_margin_top;
    }
    collapse_margins_pair(ctx.previous_bottom_margin, child_margin_top)
}

/// Spec: §8.3.1 — Compute the y position for a child from origin, cursor, and collapsed top.
#[inline]
pub const fn compute_y_position_public(
    origin_y: i32,
    cursor: i32,
    collapsed_vertical_margin: i32,
) -> i32 {
    origin_y
        .saturating_add(cursor)
        .saturating_add(collapsed_vertical_margin)
}

/// Spec: §8.3.1 — Public wrapper for effective top margin of a child.
#[inline]
pub fn effective_child_top_margin_public(
    layouter: &Layouter,
    child_key: NodeKey,
    child_sides: &BoxSides,
) -> i32 {
    effective_child_top_margin(layouter, child_key, child_sides)
}

/// Spec: §8.3.1 — Public wrapper for effective bottom margin of a child.
#[inline]
pub fn effective_child_bottom_margin_public(
    layouter: &Layouter,
    child_key: NodeKey,
    child_sides: &BoxSides,
) -> i32 {
    effective_child_bottom_margin(layouter, child_key, child_sides)
}

/// Spec: §8.3.1 — Collapse a list of vertical margins (algebraic sum of extremes).
#[allow(
    dead_code,
    reason = "Public API kept for direct spec-mapped calls; used by tests/future orchestrator paths"
)]
#[inline]
pub fn collapse_margins_list(margins: &[i32]) -> i32 {
    if margins.is_empty() {
        return 0i32;
    }
    let mut max_pos = i32::MIN;
    let mut min_neg = i32::MAX;
    let mut any_pos = false;
    let mut any_neg = false;
    for &margin in margins {
        if margin >= 0i32 {
            any_pos = true;
            if margin > max_pos {
                max_pos = margin;
            }
        } else {
            any_neg = true;
            if margin < min_neg {
                min_neg = margin;
            }
        }
    }
    match (any_pos, any_neg) {
        (true, false) => max_pos,
        (false, true) => min_neg,
        (true, true) => max_pos.saturating_add(min_neg),
        (false, false) => 0i32,
    }
}

/// Spec: §8.3.1 — Compute final outgoing bottom margin for a child box.
#[allow(
    dead_code,
    reason = "Public API kept for direct spec-mapped calls; used by tests/future orchestrator paths"
)]
#[inline]
pub fn compute_margin_bottom_out(margin_top: i32, effective_bottom: i32, is_empty: bool) -> i32 {
    if is_empty {
        collapse_margins_pair(margin_top, effective_bottom)
    } else {
        effective_bottom
    }
}

/// Spec: §8.3.1 — Outgoing bottom margin for first placed empty child.
#[allow(
    dead_code,
    reason = "Public API kept for direct spec-mapped calls; used by tests/future orchestrator paths"
)]
#[inline]
pub fn compute_first_placed_empty_margin_bottom(
    previous_bottom: i32,
    parent_self_top: i32,
    child_top_eff: i32,
    child_bottom_eff: i32,
) -> i32 {
    let list = [
        previous_bottom,
        parent_self_top,
        child_top_eff,
        child_bottom_eff,
    ];
    collapse_margins_list(&list)
}

/// Spec: §8.3.1 — Leading top collapse application at parent's top edge.
#[inline]
pub fn apply_leading_top_collapse_public(
    layouter: &Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
    block_children: &[NodeKey],
    ancestor_applied_at_edge: bool,
) -> (i32, i32, i32, usize) {
    apply_leading_top_collapse(
        layouter,
        root,
        metrics,
        block_children,
        ancestor_applied_at_edge,
    )
}

/// Heuristic structural emptiness used during leading group pre-scan.
#[inline]
fn is_structurally_empty_chain(layouter: &Layouter, start: NodeKey) -> bool {
    let mut current = start;
    loop {
        let style = layouter
            .computed_styles
            .get(&current)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        if establishes_block_formatting_context(&style) {
            debug!("[VERT-EMPTY diag node={current:?}] break: establishes BFC");
            return false;
        }
        let sides = compute_box_sides(&style);
        if style.height.unwrap_or(0.0) as i32 > 0 {
            return false;
        }
        if style.min_height.unwrap_or(0.0) as i32 > 0 {
            return false;
        }
        if has_inline_text_descendant(layouter, current) {
            return false;
        }
        if sides.padding_top != 0
            || sides.border_top != 0
            || sides.padding_bottom != 0
            || sides.border_bottom != 0
        {
            return false;
        }
        match first_block_child(layouter, current) {
            None => return true,
            Some(next) => {
                current = next;
            }
        }
    }
}

/// Compute the effective top margin for a child by collapsing with its first block descendant
/// chain when allowed by padding/border edges and structural emptiness rules.
#[inline]
fn effective_child_top_margin(
    layouter: &Layouter,
    child_key: NodeKey,
    child_sides: &BoxSides,
) -> i32 {
    let mut margins: Vec<i32> = vec![child_sides.margin_top];
    let mut current = child_key;
    let mut current_sides = *child_sides;
    let cur_style = layouter
        .computed_styles
        .get(&current)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    if establishes_block_formatting_context(&cur_style) {
        return collapse_margins_list(&margins);
    }
    while current_sides.padding_top == 0i32
        && current_sides.border_top == 0i32
        && let Some(first_desc) = first_block_child(layouter, current)
        && first_desc != current
    {
        let first_style = layouter
            .computed_styles
            .get(&first_desc)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        if establishes_block_formatting_context(&first_style) {
            break;
        }
        if is_structurally_empty_chain(layouter, current) {
            let eff_bottom = effective_child_bottom_margin(layouter, current, &current_sides);
            margins.push(eff_bottom);
        }
        let first_sides = compute_box_sides(&first_style);
        margins.push(first_sides.margin_top);
        current = first_desc;
        current_sides = first_sides;
    }
    collapse_margins_list(&margins)
}

/// Compute the effective bottom margin for a child by collapsing with its last block descendant
/// chain when allowed by padding/border edges and structural emptiness rules.
#[inline]
fn effective_child_bottom_margin(
    layouter: &Layouter,
    child_key: NodeKey,
    child_sides: &BoxSides,
) -> i32 {
    let mut margins: Vec<i32> = vec![child_sides.margin_bottom];
    let mut current = child_key;
    let mut current_sides = *child_sides;
    let style = layouter
        .computed_styles
        .get(&current)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    if establishes_block_formatting_context(&style) {
        return collapse_margins_list(&margins);
    }
    while current_sides.padding_bottom == 0i32
        && current_sides.border_bottom == 0i32
        && !has_inline_text_descendant(layouter, current)
        && let Some(last_desc) = find_last_block_under(layouter, current)
        && last_desc != current
    {
        let last_style = layouter
            .computed_styles
            .get(&last_desc)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        if establishes_block_formatting_context(&last_style) {
            break;
        }
        let last_sides = compute_box_sides(&last_style);
        margins.push(last_sides.margin_bottom);
        current = last_desc;
        current_sides = last_sides;
    }
    collapse_margins_list(&margins)
}

/// Return the first block child of a node after display tree flattening.
#[inline]
fn first_block_child(layouter: &Layouter, key: NodeKey) -> Option<NodeKey> {
    let flattened = normalize_children(&layouter.children, &layouter.computed_styles, key);
    for child in flattened {
        if matches!(
            layouter.nodes.get(&child),
            Some(LayoutNodeKind::Block { .. })
        ) {
            return Some(child);
        }
    }
    None
}

/// Find the last block descendant under a node using a flattened traversal.
#[inline]
fn find_last_block_under(layouter: &Layouter, start: NodeKey) -> Option<NodeKey> {
    let mut last: Option<NodeKey> = None;
    let flattened = normalize_children(&layouter.children, &layouter.computed_styles, start);
    for child_key in flattened {
        if let Some(found) = find_last_block_under(layouter, child_key) {
            last = Some(found);
        } else if matches!(
            layouter.nodes.get(&child_key),
            Some(&LayoutNodeKind::Block { .. })
        ) {
            last = Some(child_key);
        }
    }
    if last.is_none()
        && matches!(
            layouter.nodes.get(&start),
            Some(&LayoutNodeKind::Block { .. })
        )
    {
        last = Some(start);
    }
    last
}

/// Return true if there is any inline text descendant under the given node.
#[inline]
fn has_inline_text_descendant(layouter: &Layouter, key: NodeKey) -> bool {
    let top = normalize_children(&layouter.children, &layouter.computed_styles, key);
    for child in top {
        if matches!(
            layouter.nodes.get(&child),
            Some(&LayoutNodeKind::InlineText { .. })
        ) {
            return true;
        }
        if matches!(
            layouter.nodes.get(&child),
            Some(&LayoutNodeKind::Block { .. })
        ) {
            let nested = normalize_children(&layouter.children, &layouter.computed_styles, child);
            if nested.into_iter().any(|nxt| {
                matches!(
                    layouter.nodes.get(&nxt),
                    Some(&LayoutNodeKind::InlineText { .. })
                )
            }) {
                return true;
            }
        }
    }
    false
}

/// Scan the leading group to collect margins and skipping information used to apply or forward
/// collapse at the parent's top edge.
#[inline]
fn scan_leading_group(
    layouter: &Layouter,
    root: NodeKey,
    _metrics: &ContainerMetrics,
    block_children: &[NodeKey],
    ancestor_applied_at_edge: bool,
) -> (Vec<i32>, usize, bool, bool) {
    let parent_style = layouter
        .computed_styles
        .get(&root)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let parent_sides = compute_box_sides(&parent_style);
    let parent_edge_collapsible = parent_sides.padding_top == 0i32
        && parent_sides.border_top == 0i32
        && !establishes_block_formatting_context(&parent_style);
    let include_parent_edge = parent_edge_collapsible && !ancestor_applied_at_edge;
    debug!(
        "[VERT-GROUP pre root={root:?}] parent_edge_collapsible={parent_edge_collapsible} ancestor_applied_at_edge={ancestor_applied_at_edge} -> include_parent_edge={include_parent_edge}"
    );
    let mut leading_margins: Vec<i32> = if include_parent_edge {
        vec![parent_sides.margin_top]
    } else {
        Vec::new()
    };
    let mut skip_count: usize = 0;
    let mut idx: usize = 0;
    while let Some(child_key) = block_children.get(idx).copied() {
        let child_style = layouter
            .computed_styles
            .get(&child_key)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let child_sides = compute_box_sides(&child_style);
        let eff_top = effective_child_top_margin(layouter, child_key, &child_sides);
        let is_leading_empty = is_structurally_empty_chain(layouter, child_key);
        if is_leading_empty {
            debug!(
                "[VERT-GROUP empty] idx={} child={child_key:?} eff_top={} child_mb={} (pad_top={} border_top={} pad_bottom={} border_bottom={})",
                idx,
                eff_top,
                child_sides.margin_bottom,
                child_sides.padding_top,
                child_sides.border_top,
                child_sides.padding_bottom,
                child_sides.border_bottom
            );
            let eff_bottom = effective_child_bottom_margin(layouter, child_key, &child_sides);
            leading_margins.push(eff_top);
            leading_margins.push(eff_bottom);
            skip_count = skip_count.saturating_add(1);
            idx = idx.saturating_add(1);
            continue;
        }
        let has_clear = matches!(child_style.clear, Clear::Left | Clear::Right | Clear::Both);
        if has_clear {
            debug!(
                "[VERT-GROUP first-non-empty CLEAR] idx={idx} child={child_key:?} clear={0:?} -> eff_top suppressed from leading group",
                child_style.clear
            );
        } else {
            leading_margins.push(eff_top);
            debug!(
                "[VERT-GROUP first-non-empty] idx={idx} child={child_key:?} include_parent_edge={include_parent_edge} parent_edge_collapsible={parent_edge_collapsible} eff_top_added={eff_top}"
            );
        }
        break;
    }
    debug!(
        "[VERT-GROUP out root={root:?}] include_parent_edge={include_parent_edge} parent_edge_collapsible={parent_edge_collapsible} leading_margins={leading_margins:?} skipped={skip_count}"
    );
    (
        leading_margins,
        skip_count,
        include_parent_edge,
        parent_edge_collapsible,
    )
}

/// Apply the leading-top collapsed value at the parent's top edge when eligible, otherwise forward
/// the collapsed value to the first non-empty child.
#[inline]
const fn apply_or_forward_group(
    include_parent_edge: bool,
    parent_edge_collapsible: bool,
    leading_top: i32,
) -> (i32, i32, i32) {
    if include_parent_edge {
        return (leading_top, 0, leading_top);
    }
    if !parent_edge_collapsible {
        return (0, leading_top, 0);
    }
    (0, leading_top, 0)
}

/// Apply the leading-top collapse rules (parent edge vs forwarding to the first non-empty child)
/// and return (`y_start`, `prev_bottom_after`, `leading_applied`, `skip_count`).
#[inline]
fn apply_leading_top_collapse(
    layouter: &Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
    block_children: &[NodeKey],
    ancestor_applied_at_edge: bool,
) -> (i32, i32, i32, usize) {
    if block_children.is_empty() {
        return (0, 0, 0, 0);
    }
    let (leading_margins, skip_count, include_parent_edge, parent_edge_collapsible) =
        scan_leading_group(
            layouter,
            root,
            metrics,
            block_children,
            ancestor_applied_at_edge,
        );
    let leading_top = collapse_margins_list(&leading_margins);
    let (y_start, prev_bottom_after, leading_applied) =
        apply_or_forward_group(include_parent_edge, parent_edge_collapsible, leading_top);
    (y_start, prev_bottom_after, leading_applied, skip_count)
}

/// Spec: §8.3.1 — Compute the root y after top collapse with first child when applicable.
#[inline]
pub fn compute_root_y_after_top_collapse(
    layouter: &Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
) -> i32 {
    if metrics.padding_top == 0i32 && metrics.border_top == 0i32 {
        let flattened = normalize_children(&layouter.children, &layouter.computed_styles, root);
        if let Some(first_child) = flattened
            .into_iter()
            .find(|key| matches!(layouter.nodes.get(key), Some(&LayoutNodeKind::Block { .. })))
        {
            let first_style = layouter
                .computed_styles
                .get(&first_child)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let first_sides = compute_box_sides(&first_style);
            let first_effective_top =
                effective_child_top_margin(layouter, first_child, &first_sides);
            let collapsed = collapse_margins_pair(metrics.margin_top, first_effective_top);
            debug!(
                "[ROOT-COLLAPSE] root={root:?} metrics(mt={}, pt={}, bt={}) first_child={first_child:?} eff_top={} -> collapsed_top={}",
                metrics.margin_top,
                metrics.padding_top,
                metrics.border_top,
                first_effective_top,
                collapsed
            );
            return collapsed.max(0);
        }
    }
    metrics.margin_top
}
