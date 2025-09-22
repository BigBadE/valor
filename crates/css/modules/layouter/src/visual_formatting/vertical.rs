//! Vertical margin collapsing per CSS 2.2 §8.3.1
//!
//! Implements the leading-top margin collapse group computation and
//! application rules for block formatting contexts.
//!
//! Spec: <https://www.w3.org/TR/CSS22/box.html#collapsing-margins>

use css_box::{BoxSides, compute_box_sides};
use log::debug;
use style_engine::ComputedStyle;

use crate::{ContainerMetrics, LayoutNodeKind, Layouter};
use js::NodeKey;

/// Heuristic structural emptiness used during leading group pre-scan (CSS §8.3.1).
///
/// Walks a chain of first block children while each box has zero top/bottom padding and border.
/// If the chain terminates without another block child under those constraints, treat as empty.
#[inline]
pub fn is_structurally_empty_chain(layouter: &Layouter, start: NodeKey) -> bool {
    let mut current = start;
    loop {
        let style = layouter
            .computed_styles
            .get(&current)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let sides = compute_box_sides(&style);
        if style.height.unwrap_or(0.0) as i32 > 0 {
            debug!("[VERT-EMPTY diag node={current:?}] break: explicit height>0");
            return false;
        }
        if style.min_height.unwrap_or(0.0) as i32 > 0 {
            debug!("[VERT-EMPTY diag node={current:?}] break: min-height>0");
            return false;
        }
        if has_inline_text_descendant(layouter, current) {
            debug!("[VERT-EMPTY diag node={current:?}] break: has inline text descendant");
            return false;
        }
        if sides.padding_top != 0
            || sides.border_top != 0
            || sides.padding_bottom != 0
            || sides.border_bottom != 0
        {
            debug!(
                "[VERT-EMPTY diag node={current:?}] break: non-zero padding/border (pt={},bt={},pb={},bb={})",
                sides.padding_top, sides.border_top, sides.padding_bottom, sides.border_bottom
            );
            return false;
        }
        match first_block_child(layouter, current) {
            None => {
                debug!("[VERT-EMPTY diag node={current:?}] end-of-chain: returns true");
                return true;
            }
            Some(next) => {
                debug!("[VERT-EMPTY diag node={current:?}] continue -> first_block_child={next:?}");
                current = next;
            }
        }
    }
}

/// Compute an effective top margin for a child, collapsing with its first block child's top margin.
///
/// Applies when the child has no top padding/border and contains no inline text (CSS 2.2 §8.3.1 approximation).
#[inline]
pub fn effective_child_top_margin(
    layouter: &Layouter,
    child_key: NodeKey,
    child_sides: &BoxSides,
) -> i32 {
    let mut margins: Vec<i32> = vec![child_sides.margin_top];
    let mut current = child_key;
    let mut current_sides = *child_sides;
    while current_sides.padding_top == 0i32
        && current_sides.border_top == 0i32
        && !has_inline_text_descendant(layouter, current)
        && let Some(first_desc) = first_block_child(layouter, current)
        && first_desc != current
    {
        let first_style = layouter
            .computed_styles
            .get(&first_desc)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let first_sides = compute_box_sides(&first_style);
        margins.push(first_sides.margin_top);
        current = first_desc;
        current_sides = first_sides;
    }
    Layouter::collapse_margins_list(&margins)
}

/// Compute an effective bottom margin for a child, collapsing with its last block child's bottom margin.
///
/// Applies when the child has no bottom padding/border and contains no inline text (CSS 2.2 §8.3.1 approximation).
#[inline]
pub fn effective_child_bottom_margin(
    layouter: &Layouter,
    child_key: NodeKey,
    child_sides: &BoxSides,
) -> i32 {
    let mut margins: Vec<i32> = vec![child_sides.margin_bottom];
    let mut current = child_key;
    let mut current_sides = *child_sides;
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
        let last_sides = compute_box_sides(&last_style);
        margins.push(last_sides.margin_bottom);
        current = last_desc;
        current_sides = last_sides;
    }
    Layouter::collapse_margins_list(&margins)
}

#[inline]
/// Return the first block child of `key`, if any.
fn first_block_child(layouter: &Layouter, key: NodeKey) -> Option<NodeKey> {
    let kids = layouter.children.get(&key)?;
    kids.iter().copied().find(|node_key| {
        matches!(
            layouter.nodes.get(node_key),
            Some(LayoutNodeKind::Block { .. })
        )
    })
}

#[inline]
/// Depth-first search for the last block-level descendant under `start`.
fn find_last_block_under(layouter: &Layouter, start: NodeKey) -> Option<NodeKey> {
    let mut last: Option<NodeKey> = None;
    if let Some(child_list) = layouter.children.get(&start) {
        for child_key in child_list {
            if let Some(found) = find_last_block_under(layouter, *child_key) {
                last = Some(found);
            } else if matches!(
                layouter.nodes.get(child_key),
                Some(LayoutNodeKind::Block { .. })
            ) {
                last = Some(*child_key);
            }
        }
    }
    if last.is_none()
        && matches!(
            layouter.nodes.get(&start),
            Some(LayoutNodeKind::Block { .. })
        )
    {
        last = Some(start);
    }
    last
}

#[inline]
/// Returns true if any inline text descendant exists beneath `key`.
fn has_inline_text_descendant(layouter: &Layouter, key: NodeKey) -> bool {
    let mut stack: Vec<NodeKey> = match layouter.children.get(&key) {
        Some(kids) => kids.clone(),
        None => return false,
    };
    while let Some(current) = stack.pop() {
        let node_kind = layouter.nodes.get(&current).cloned();
        if matches!(node_kind, Some(LayoutNodeKind::InlineText { .. })) {
            return true;
        }
        if matches!(
            node_kind,
            Some(LayoutNodeKind::Block { .. } | LayoutNodeKind::Document)
        ) && let Some(children) = layouter.children.get(&current)
        {
            stack.extend(children.iter().copied());
        }
    }
    false
}

#[inline]
/// Scan the leading collapsible group and collect margins.
/// Returns `(margins, skip_count, include_parent_edge, parent_edge_collapsible)`.
fn scan_leading_group(
    layouter: &Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
    block_children: &[NodeKey],
    ancestor_applied_at_edge: bool,
) -> (Vec<i32>, usize, bool, bool) {
    let parent_style = layouter
        .computed_styles
        .get(&root)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let parent_sides = compute_box_sides(&parent_style);
    let parent_edge_collapsible = metrics.padding_top == 0i32 && metrics.border_top == 0i32;
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
        debug!(
            "[VERT-GROUP scan root={root:?}] child={child_key:?} eff_top={eff_top} paddings(top={},bottom={}) borders(top={},bottom={}) height={:?} structurally_empty_chain={}",
            child_sides.padding_top,
            child_sides.padding_bottom,
            child_sides.border_top,
            child_sides.border_bottom,
            child_style.height,
            is_leading_empty
        );
        if is_leading_empty {
            let eff_bottom = effective_child_bottom_margin(layouter, child_key, &child_sides);
            debug!(
                "[VERT-GROUP scan-empty root={root:?}] child={child_key:?} push(eff_top={}, eff_bottom={})",
                eff_top, eff_bottom
            );
            leading_margins.push(eff_top);
            leading_margins.push(eff_bottom);
            skip_count = skip_count.saturating_add(1);
            idx = idx.saturating_add(1);
            continue;
        }
        debug!(
            "[VERT-GROUP scan-stop root={root:?}] child={child_key:?} non-empty push(eff_top={})",
            eff_top
        );
        leading_margins.push(eff_top);
        break;
    }
    (
        leading_margins,
        skip_count,
        include_parent_edge,
        parent_edge_collapsible,
    )
}

#[inline]
/// Decide where the leading group applies and return
/// `(y_cursor_start, previous_bottom_after, leading_top_applied)`.
fn apply_or_forward_group(
    include_parent_edge: bool,
    parent_edge_collapsible: bool,
    leading_top: i32,
) -> (i32, i32, i32) {
    if include_parent_edge {
        let out = (leading_top.max(0i32), 0i32, leading_top);
        debug!("[VERT-GROUP apply-at-parent] out={out:?}");
        return out;
    }
    if !parent_edge_collapsible {
        let out = (0i32, leading_top, 0i32);
        debug!("[VERT-GROUP forward-internal blocked-parent-edge] out={out:?}");
        return out;
    }
    let out = (0i32, leading_top, 0i32);
    debug!("[VERT-GROUP forward-internal ancestor-already-applied] out={out:?}");
    out
}

/// Compute and apply the leading-empty-chain collapse per CSS §8.3.1.
///
/// Returns (`y_cursor_start`, `previous_bottom_margin`, `leading_top_applied`, `skip_count`).
#[inline]
pub fn apply_leading_top_collapse(
    layouter: &Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
    block_children: &[NodeKey],
    ancestor_applied_at_edge: bool,
) -> (i32, i32, i32, usize) {
    if block_children.is_empty() {
        debug!("[VERT-GROUP root={root:?}] skip pre-scan: empty children");
        return (0i32, 0i32, 0i32, 0usize);
    }
    let (leading_margins, skip_count, include_parent_edge, parent_edge_collapsible) =
        scan_leading_group(
            layouter,
            root,
            metrics,
            block_children,
            ancestor_applied_at_edge,
        );
    if skip_count == 0
        && leading_margins.is_empty()
        && !include_parent_edge
        && parent_edge_collapsible
    {
        return (0i32, 0i32, 0i32, 0usize);
    }
    let leading_top = Layouter::collapse_margins_list(&leading_margins);
    debug!(
        "[VERT-GROUP root={root:?}] include_parent_edge={include_parent_edge} parent_edge_collapsible={parent_edge_collapsible} ancestor_applied={ancestor_applied_at_edge} leading_skip={skip_count} margins={leading_margins:?} -> leading_top={leading_top}"
    );
    let (y_start, prev_bottom_after, leading_applied) =
        apply_or_forward_group(include_parent_edge, parent_edge_collapsible, leading_top);
    (y_start, prev_bottom_after, leading_applied, skip_count)
}
