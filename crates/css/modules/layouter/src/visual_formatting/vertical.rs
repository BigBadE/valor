//! Vertical margin collapsing per CSS 2.2 §8.3.1
//!
//! Implements the leading-top margin collapse group computation and
//! application rules for block formatting contexts.
//!
//! Spec: <https://www.w3.org/TR/CSS22/box.html#collapsing-margins>

use css_box::{BoxSides, compute_box_sides};
use log::debug;
use style_engine::{Clear, ComputedStyle, Float, Overflow, Position};

use crate::{ContainerMetrics, LayoutNodeKind, Layouter, box_tree};
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
    // Do not propagate through a BFC boundary (e.g., overflow other than visible).
    let cur_style = layouter
        .computed_styles
        .get(&current)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    if establishes_bfc(&cur_style) {
        return Layouter::collapse_margins_list(&margins);
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
        if establishes_bfc(&first_style) {
            // Stop propagation at BFC boundary.
            break;
        }
        // If the current box is structurally empty, its top and bottom margins
        // collapse together and propagate to its first block descendant per §8.3.1.
        // Include the current box's effective bottom margin before descending so the
        // accumulated list reflects the empty chain correctly (e.g., 0 + 12 -> 12).
        if is_structurally_empty_chain(layouter, current) {
            let eff_bottom = effective_child_bottom_margin(layouter, current, &current_sides);
            margins.push(eff_bottom);
        }
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
    // Do not propagate through a BFC boundary at the descendant edge.
    if establishes_bfc(
        &layouter
            .computed_styles
            .get(&current)
            .cloned()
            .unwrap_or_else(ComputedStyle::default),
    ) {
        return Layouter::collapse_margins_list(&margins);
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
        if establishes_bfc(&last_style) {
            break;
        }
        let last_sides = compute_box_sides(&last_style);
        margins.push(last_sides.margin_bottom);
        current = last_desc;
        current_sides = last_sides;
    }
    Layouter::collapse_margins_list(&margins)
}

#[inline]
/// Minimal BFC heuristic used by margin-collapsing traversal.
const fn establishes_bfc(style: &ComputedStyle) -> bool {
    // Establish a BFC if any of the following are true:
    // - overflow is not Visible
    // - float is not None
    // - position is not Static (absolute/fixed/sticky)
    if !matches!(style.overflow, Overflow::Visible) {
        return true;
    }
    if !matches!(style.float, Float::None) {
        return true;
    }
    if !matches!(style.position, Position::Static) {
        return true;
    }
    false
}

#[inline]
/// Return the first block child of `key`, if any.
fn first_block_child(layouter: &Layouter, key: NodeKey) -> Option<NodeKey> {
    // Walk the flattened display children to honor display:contents passthrough.
    let flattened =
        box_tree::flatten_display_children(&layouter.children, &layouter.computed_styles, key);
    flattened.into_iter().find(|node_key| {
        matches!(
            layouter.nodes.get(node_key),
            Some(&LayoutNodeKind::Block { .. })
        )
    })
}

#[inline]
/// Depth-first search for the last block-level descendant under `start`.
fn find_last_block_under(layouter: &Layouter, start: NodeKey) -> Option<NodeKey> {
    let mut last: Option<NodeKey> = None;
    let flattened =
        box_tree::flatten_display_children(&layouter.children, &layouter.computed_styles, start);
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

#[inline]
/// Returns true if any inline text descendant exists beneath `key`.
fn has_inline_text_descendant(layouter: &Layouter, key: NodeKey) -> bool {
    // Use flattened display traversal so display:contents does not falsely separate chains.
    let mut stack: Vec<NodeKey> =
        box_tree::flatten_display_children(&layouter.children, &layouter.computed_styles, key);
    while let Some(current) = stack.pop() {
        let node_kind = layouter.nodes.get(&current).cloned();
        if matches!(node_kind, Some(LayoutNodeKind::InlineText { .. })) {
            return true;
        }
        if matches!(
            node_kind,
            Some(LayoutNodeKind::Block { .. } | LayoutNodeKind::Document)
        ) {
            let mut flattened = box_tree::flatten_display_children(
                &layouter.children,
                &layouter.computed_styles,
                current,
            );
            stack.append(&mut flattened);
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
                "[VERT-GROUP scan-empty root={root:?}] child={child_key:?} push(eff_top={eff_top}, eff_bottom={eff_bottom})"
            );
            leading_margins.push(eff_top);
            leading_margins.push(eff_bottom);
            skip_count = skip_count.saturating_add(1);
            idx = idx.saturating_add(1);
            continue;
        }
        // If the first non-empty child has clear, its top margin does not collapse with the
        // previous sibling and should not be absorbed at the parent edge.
        let has_clear = matches!(child_style.clear, Clear::Left | Clear::Right | Clear::Both);
        debug!(
            "[VERT-GROUP scan-stop root={root:?}] child={child_key:?} non-empty eff_top={eff_top} clear={has_clear}"
        );
        if !has_clear {
            leading_margins.push(eff_top);
        }
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
        let out = (leading_top, 0i32, leading_top);
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
