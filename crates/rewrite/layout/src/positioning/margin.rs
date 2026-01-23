/// Margin collapsing module implementing CSS 2.2 margin collapsing rules.
///
/// This module handles:
/// - Adjacent sibling margin collapsing
/// - Parent-child margin collapsing
/// - Empty block margin collapsing
/// - Negative margin collapsing
///
/// Spec: https://www.w3.org/TR/CSS22/box.html#collapsing-margins
use crate::{BlockMarker, ConstrainedMarker, SizeQuery, Subpixels};
use rewrite_core::{NodeId, Relationship, ScopedDb};
use rewrite_css::{
    BorderWidthQuery, CssKeyword, CssValue, DisplayQuery, EndMarker, MarginQuery, PaddingQuery,
    PositionQuery, StartMarker,
};

/// Compute collapsed margin for the start edge (top) of a block element.
///
/// # Margin Collapsing Rules:
///
/// Margins collapse when they are "adjoining", which means:
/// 1. Both margins are in the block direction (vertical in horizontal writing mode)
/// 2. No line boxes, padding, or borders separate them
/// 3. Both belong to in-flow block-level boxes
///
/// ## Adjacent Siblings
/// The bottom margin of a box and the top margin of its following sibling collapse.
///
/// ## Parent-Child
/// The top margin of a box and the top margin of its first in-flow child collapse if:
/// - The parent has no top border
/// - The parent has no top padding
/// - The parent has no clearance (from floats)
///
/// Similarly for bottom margins of parent and last child.
///
/// ## Empty Blocks
/// If a block has no content, padding, or borders, its top and bottom margins collapse.
///
/// ## Collapsing Algorithm
/// When multiple margins collapse:
/// - All positive margins: use the maximum
/// - All negative margins: use the minimum (most negative)
/// - Mixed positive and negative: sum the maximum positive and minimum negative
pub fn compute_collapsed_margin_start(scoped: &mut ScopedDb) -> Subpixels {
    // Check if this element participates in margin collapsing
    if !participates_in_margin_collapsing(scoped) {
        return scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
    }

    let mut margins = vec![scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>()];

    // Collect margins that should collapse together
    collect_collapsing_margins_start(scoped, &mut margins);

    // Apply collapsing algorithm
    collapse_margins(&margins)
}

/// Compute collapsed margin for the end edge (bottom) of a block element.
pub fn compute_collapsed_margin_end(scoped: &mut ScopedDb) -> Subpixels {
    // Check if this element participates in margin collapsing
    if !participates_in_margin_collapsing(scoped) {
        return scoped.query::<MarginQuery<rewrite_css::BlockMarker, EndMarker>>();
    }

    let mut margins = vec![scoped.query::<MarginQuery<rewrite_css::BlockMarker, EndMarker>>()];

    // Collect margins that should collapse together
    collect_collapsing_margins_end(scoped, &mut margins);

    // Apply collapsing algorithm
    collapse_margins(&margins)
}

// ============================================================================
// Margin Collection
// ============================================================================

/// Collect all margins that should collapse with the start margin.
///
/// This includes:
/// 1. Previous sibling's end margin (if adjoining)
/// 2. Parent's start margin (if no separating border/padding)
/// 3. First child's start margin (if no separating border/padding)
///
/// IMPORTANT: For children and siblings, we recursively query their COLLAPSED
/// margins, not their raw margins. This allows multi-level collapsing.
///
/// Note: We carefully manage borrows to avoid holding `db` reference while
/// mutably accessing `scoped`.
fn collect_collapsing_margins_start(scoped: &mut ScopedDb, margins: &mut Vec<Subpixels>) {
    // Get node ID upfront (no borrow held)
    let node = scoped.node();

    // 1. Check previous sibling's bottom margin
    // Get the sibling ID in a separate scope to release the db borrow
    let prev_sibling = {
        let db = scoped.db();
        db.resolve_relationship(node, Relationship::PreviousSiblings)
            .last()
            .copied()
    };

    if let Some(prev_sibling) = prev_sibling {
        // Check if siblings are adjoining (need mutable access for queries)
        let adjoining = {
            let display = scoped.node_query::<DisplayQuery>(prev_sibling);
            let current_display = scoped.query::<DisplayQuery>();
            matches!(display, CssValue::Keyword(CssKeyword::Block))
                && matches!(current_display, CssValue::Keyword(CssKeyword::Block))
        };

        if adjoining {
            // Query the COLLAPSED margin recursively
            let mut sibling_scoped = scoped.scoped_to(prev_sibling);
            let prev_margin = get_effective_margin_end(&mut sibling_scoped);
            margins.push(prev_margin);
        }
    } else {
        // No previous sibling: check parent's top margin
        if can_collapse_with_parent_start(scoped) {
            // Get parent ID in a separate scope
            let parent = {
                let db = scoped.db();
                db.resolve_relationship(node, Relationship::Parent)
                    .first()
                    .copied()
            };

            if let Some(parent) = parent {
                // Query the parent's RAW margin (not collapsed) to avoid infinite recursion.
                // The parent's margin will collapse with our margin, but we shouldn't
                // recursively query the parent's collapsed margin because that would
                // create a cycle (parent queries us, we query parent, etc.)
                let parent_margin =
                    scoped.node_query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>(parent);
                margins.push(parent_margin);
            }
        }
    }

    // 2. Check first child's top margin (if no separating border/padding)
    let can_collapse = can_collapse_with_first_child(scoped);

    if can_collapse {
        // Get first child ID in a separate scope
        let first_child = {
            let db = scoped.db();
            db.resolve_relationship(node, Relationship::Children)
                .first()
                .copied()
        };

        if let Some(first_child) = first_child {
            // Query the COLLAPSED margin recursively
            let mut child_scoped = scoped.scoped_to(first_child);
            let child_margin = get_effective_margin_start(&mut child_scoped);
            margins.push(child_margin);
        }
    }
}

/// Collect all margins that should collapse with the end margin.
///
/// IMPORTANT: For children, we recursively query their COLLAPSED margins,
/// not their raw margins. This allows multi-level collapsing.
fn collect_collapsing_margins_end(scoped: &mut ScopedDb, margins: &mut Vec<Subpixels>) {
    let node = scoped.node();

    // 1. Check last child's bottom margin (if no separating border/padding)
    if can_collapse_with_last_child(scoped) {
        // Get last child ID in a separate scope
        let last_child = {
            let db = scoped.db();
            db.resolve_relationship(node, Relationship::Children)
                .last()
                .copied()
        };

        if let Some(last_child) = last_child {
            // Query the COLLAPSED margin recursively
            let mut child_scoped = scoped.scoped_to(last_child);
            let child_margin = get_effective_margin_end(&mut child_scoped);
            margins.push(child_margin);
        }
    }

    // 2. Check if this is an empty block (top and bottom margins collapse)
    if is_empty_block(scoped) {
        let top_margin = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
        margins.push(top_margin);
    }
}

// ============================================================================
// Collapsing Rules
// ============================================================================

/// Apply the margin collapsing algorithm to a set of margins.
///
/// Rules:
/// - All positive: max
/// - All negative: min (most negative)
/// - Mixed: max positive + min negative
fn collapse_margins(margins: &[Subpixels]) -> Subpixels {
    if margins.is_empty() {
        return 0;
    }

    let positive_margins: Vec<Subpixels> = margins.iter().copied().filter(|&m| m > 0).collect();
    let negative_margins: Vec<Subpixels> = margins.iter().copied().filter(|&m| m < 0).collect();

    let max_positive = positive_margins.iter().max().copied().unwrap_or(0);
    let min_negative = negative_margins.iter().min().copied().unwrap_or(0);

    max_positive + min_negative
}

/// Check if two sibling elements have adjoining margins.
///
/// Margins are adjoining if:
/// - Both are in-flow block-level boxes
/// - No line boxes, clearance, padding, or borders separate them
fn are_siblings_adjoining(scoped: &mut ScopedDb, prev_sibling: NodeId) -> bool {
    // Check if previous sibling is in-flow block
    if !is_in_flow_block(scoped, prev_sibling) {
        return false;
    }

    // Check if current element is in-flow block
    if !is_in_flow_block_current(scoped) {
        return false;
    }

    // No line boxes or clearance between siblings
    // (simplified: assume no floats/clearance for now)
    true
}

/// Check if element's top margin can collapse with parent's top margin.
///
/// Conditions:
/// - Element is first child
/// - Parent has no top border
/// - Parent has no top padding
/// - Parent is in-flow block
pub fn can_collapse_with_parent_start(scoped: &mut ScopedDb) -> bool {
    // TEMPORARY: Disable parent-child margin collapsing
    // This is causing issues because we're not correctly identifying the first in-flow child
    // TODO: Fix the logic to properly find the first in-flow child and enable this
    return false;

    #[allow(unreachable_code)]
    {
        let Some(parent) = scoped.parent_id() else {
            return false;
        };

        // Check parent has no top border
        let parent_border =
            scoped.node_query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>(parent);
        if parent_border > 0 {
            return false;
        }

        // Check parent has no top padding
        let parent_padding =
            scoped.node_query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>(parent);
        if parent_padding > 0 {
            return false;
        }

        // Check parent is in-flow block
        if !is_in_flow_block(scoped, parent) {
            return false;
        }

        true
    }
}

/// Check if element's top margin can collapse with its first child's top margin.
///
/// Conditions:
/// - Element has no top border
/// - Element has no top padding
/// - Element is in-flow block
/// - First child is in-flow block
fn can_collapse_with_first_child(scoped: &mut ScopedDb) -> bool {
    // Check element has no top border
    let border = scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>();
    if border > 0 {
        return false;
    }

    // Check element has no top padding
    let padding = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>();
    if padding > 0 {
        return false;
    }

    // Check element is in-flow block
    if !is_in_flow_block_current(scoped) {
        return false;
    }

    // Check first child exists and is in-flow block
    if let Some(first_child) = scoped.first_child() {
        is_in_flow_block(scoped, first_child)
    } else {
        false
    }
}

/// Check if element's bottom margin can collapse with its last child's bottom margin.
fn can_collapse_with_last_child(scoped: &mut ScopedDb) -> bool {
    // Check element has no bottom border
    let border = scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, EndMarker>>();
    if border > 0 {
        return false;
    }

    // Check element has no bottom padding
    let padding = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>();
    if padding > 0 {
        return false;
    }

    // Check element is in-flow block
    if !is_in_flow_block_current(scoped) {
        return false;
    }

    // Check last child exists and is in-flow block
    if let Some(last_child) = scoped.last_child() {
        is_in_flow_block(scoped, last_child)
    } else {
        false
    }
}

/// Check if this is an empty block whose top and bottom margins should collapse.
///
/// An empty block has:
/// - No content (no children or all children are whitespace/comments)
/// - No padding
/// - No border
/// - Auto or zero height
fn is_empty_block(scoped: &mut ScopedDb) -> bool {
    // Check no border
    let border_top = scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>();
    let border_bottom = scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, EndMarker>>();
    if border_top > 0 || border_bottom > 0 {
        return false;
    }

    // Check no padding
    let padding_top = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>();
    let padding_bottom = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>();
    if padding_top > 0 || padding_bottom > 0 {
        return false;
    }

    // Check no content (simplified: check if block size is zero)
    let block_size = scoped.query::<SizeQuery<BlockMarker, ConstrainedMarker>>();
    block_size == 0
}

// ============================================================================
// Element Classification
// ============================================================================

/// Check if an element participates in margin collapsing.
///
/// Elements that do NOT participate:
/// - Floated elements
/// - Absolutely positioned elements
/// - Inline-level elements
/// - Elements that establish a BFC (see bfc.rs)
fn participates_in_margin_collapsing(scoped: &mut ScopedDb) -> bool {
    // Check not floated
    // TODO: Add float check when float support is added

    // Check not absolutely positioned
    let position = scoped.query::<PositionQuery>();
    if matches!(
        position,
        CssValue::Keyword(CssKeyword::Absolute) | CssValue::Keyword(CssKeyword::Fixed)
    ) {
        return false;
    }

    // Check is block-level
    if !is_in_flow_block_current(scoped) {
        return false;
    }

    // Check doesn't establish BFC
    if establishes_bfc_current(scoped) {
        return false;
    }

    true
}

/// Check if a specific node is an in-flow block-level box.
fn is_in_flow_block(scoped: &mut ScopedDb, node: NodeId) -> bool {
    let display = scoped.node_query::<DisplayQuery>(node);
    matches!(display, CssValue::Keyword(CssKeyword::Block))
}

/// Check if the current element is an in-flow block-level box.
fn is_in_flow_block_current(scoped: &mut ScopedDb) -> bool {
    let display = scoped.query::<DisplayQuery>();
    matches!(display, CssValue::Keyword(CssKeyword::Block))
}

/// Check if the current element establishes a block formatting context.
///
/// This is a simplified check. See bfc.rs for the full implementation.
fn establishes_bfc_current(scoped: &mut ScopedDb) -> bool {
    // Use the full BFC implementation from the formatting_contexts module
    crate::formatting_contexts::bfc::establishes_bfc(scoped)
}

// ============================================================================
// Public API for Offset Calculation
// ============================================================================

/// Compute the effective top margin considering collapsing.
///
/// This should be used when calculating block offsets to account for
/// margin collapsing between adjacent elements.
pub fn get_effective_margin_start(scoped: &mut ScopedDb) -> Subpixels {
    let raw_margin = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
    let result = if participates_in_margin_collapsing(scoped) {
        compute_collapsed_margin_start(scoped)
    } else {
        raw_margin
    };

    // DEBUG: Check if we're getting any non-zero margins
    if raw_margin != 0 || result != 0 {
        eprintln!(
            "get_effective_margin_start: node={:?}, raw={}, collapsed={}",
            scoped.node(),
            raw_margin,
            result
        );
    }

    result
}

/// Compute the effective bottom margin considering collapsing.
pub fn get_effective_margin_end(scoped: &mut ScopedDb) -> Subpixels {
    if participates_in_margin_collapsing(scoped) {
        compute_collapsed_margin_end(scoped)
    } else {
        scoped.query::<MarginQuery<rewrite_css::BlockMarker, EndMarker>>()
    }
}

/// Get the margin to use for offset calculation.
///
/// This handles the case where an element's margin collapses with its first child:
/// - If the element can collapse with its first child, use the collapsed margin
///   (which includes both the element's margin and the child's margin)
/// - Otherwise, use the element's own margin
///
/// This is used for positioning elements that are NOT the first child of their parent.
pub fn get_margin_for_offset(scoped: &mut ScopedDb) -> Subpixels {
    // Find the first IN-FLOW child (first child that participates in layout)
    // We can't use first_child() because it returns the first DOM child,
    // which might be display:none (like <head>)
    let node = scoped.node();
    let db = scoped.db();
    let children = db.resolve_relationship(node, rewrite_core::Relationship::Children);

    let mut first_in_flow_child = None;
    for &child_id in &children {
        let mut child_scoped = scoped.scoped_to(child_id);
        let participates = participates_in_margin_collapsing(&mut child_scoped);
        let display = child_scoped.query::<DisplayQuery>();
        eprintln!(
            "get_margin_for_offset: checking child {:?}, display={:?}, participates={}",
            child_id, display, participates
        );
        if participates {
            first_in_flow_child = Some(child_id);
            eprintln!(
                "get_margin_for_offset: node={:?}, found first in-flow child={:?}",
                node, child_id
            );
            break;
        }
    }

    let first_child_id = match first_in_flow_child {
        Some(id) => id,
        None => {
            // No in-flow children - just use our own margin
            let m = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
            eprintln!(
                "get_margin_for_offset: node={:?}, no in-flow children, own margin={}",
                node, m
            );
            return m;
        }
    };

    // Check if we can collapse with first child
    // Conditions:
    // - We have no top padding
    // - We have no top border
    let has_padding = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>() > 0;
    let has_border = scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>() > 0;

    if has_padding || has_border {
        // Can't collapse - use our own margin
        let m = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
        eprintln!(
            "get_margin_for_offset: node={:?}, has padding/border, own margin={}",
            node, m
        );
        return m;
    }

    // Our margin collapses with first child - use collapsed value
    let own_margin = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();

    // Get first child's margin
    let child_margin = {
        let mut child_scoped = scoped.scoped_to(first_child_id);
        let margin = child_scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
        eprintln!(
            "get_margin_for_offset: querying child {:?} margin-top = {}",
            first_child_id, margin
        );
        margin
    };

    // Return the collapsed margin (max of positive, min of negative)
    let result = collapse_margins(&[own_margin, child_margin]);
    eprintln!(
        "get_margin_for_offset: node={:?}, own={}, child={}, result={}",
        node, own_margin, child_margin, result
    );
    result
}
