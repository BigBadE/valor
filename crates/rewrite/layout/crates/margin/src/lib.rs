//! Margin query implementation and margin collapsing logic.
//!
//! This module provides:
//! - Basic margin property queries (returns margin values or MARGIN_AUTO sentinel)
//! - CSS 2.2 margin collapsing implementation
//!
//! Margin collapsing rules:
//! - Adjacent sibling margin collapsing
//! - Parent-child margin collapsing
//! - Empty block margin collapsing
//! - Negative margin collapsing
//!
//! Spec: https://www.w3.org/TR/CSS22/box.html#collapsing-margins

use rewrite_core::{NodeDataExt, NodeId, Relationship, ScopedDb};
use rewrite_css::{Axis, LogicalDirection, Subpixels};
use rewrite_css::{CssKeyword, CssValue, DisplayQuery, EndMarker, PositionQuery, StartMarker};

/// Sentinel value indicating margin:auto (will be resolved by layout)
pub const MARGIN_AUTO: Subpixels = i32::MIN;

/// Margin property query - returns margin values or MARGIN_AUTO sentinel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(Subpixels)]
pub enum MarginProperty {
    #[query(get_margin)]
    #[params(rewrite_css::AxisMarker, rewrite_css::LogicalDirectionMarker)]
    Margin,
}

// The macro generates: pub type MarginQuery = MarginProperty_Margin;

/// Get margin for a specific axis and direction.
/// Returns MARGIN_AUTO sentinel for auto margins.
fn get_margin(scoped: &mut ScopedDb, axis: Axis, direction: LogicalDirection) -> Subpixels {
    let property = rewrite_css::margin_property_name(axis, direction);
    rewrite_css::get_dimensional_value(scoped, property).unwrap_or(MARGIN_AUTO)
}

/// Padding property query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(Subpixels)]
pub enum PaddingProperty {
    #[query(get_padding)]
    #[params(rewrite_css::AxisMarker, rewrite_css::LogicalDirectionMarker)]
    Padding,
}

// The macro generates: pub type PaddingQuery = PaddingProperty_Padding;

/// Get padding for a specific axis and direction.
fn get_padding(scoped: &mut ScopedDb, axis: Axis, direction: LogicalDirection) -> Subpixels {
    let property = rewrite_css::padding_property_name(axis, direction);
    rewrite_css::get_dimensional_value(scoped, property).unwrap_or(0)
}

/// Border width property query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(Subpixels)]
pub enum BorderWidthProperty {
    #[query(get_border_width)]
    #[params(rewrite_css::AxisMarker, rewrite_css::LogicalDirectionMarker)]
    BorderWidth,
}

// The macro generates: pub type BorderWidthQuery = BorderWidthProperty_BorderWidth;

/// Get border width for a specific axis and direction.
fn get_border_width(scoped: &mut ScopedDb, axis: Axis, direction: LogicalDirection) -> Subpixels {
    let property = rewrite_css::border_width_property_name(axis, direction);
    rewrite_css::get_dimensional_value(scoped, property).unwrap_or(0)
}

// ============================================================================
// Margin Collapsing Implementation
// ============================================================================

/// Compute the effective top margin considering collapsing.
///
/// This should be used when calculating block offsets to account for
/// margin collapsing between adjacent elements.
pub fn get_effective_margin_start(scoped: &mut ScopedDb) -> Subpixels {
    let raw_margin = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();

    // If margin is auto, it can't participate in collapsing - return 0
    // Auto margins will be resolved separately during layout
    if raw_margin == MARGIN_AUTO {
        return 0;
    }

    let participates = participates_in_margin_collapsing(scoped);

    // Check if this is the first child and can collapse with parent
    let is_first_child = {
        let db = scoped.db();
        let prev_siblings = db.resolve_relationship(scoped.node(), Relationship::PreviousSiblings);
        prev_siblings.is_empty()
    };

    // If we're the first child and can collapse with parent, return 0
    // because our margin is already included in the parent's offset calculation
    if participates && is_first_child && can_collapse_with_parent_start(scoped) {
        return 0;
    }

    let result = if participates {
        compute_collapsed_margin_start(scoped)
    } else {
        raw_margin
    };

    // DEBUG: Check if we're getting any non-zero margins
    if raw_margin != 0 || result != 0 {
        let tag = if let Some(tag) = scoped
            .db()
            .get_node_data::<rewrite_html::NodeData>(scoped.node())
        {
            match tag {
                rewrite_html::NodeData::Element(e) => format!("{}", e.tag_name),
                rewrite_html::NodeData::Text(_) => "text".to_string(),
                _ => "other".to_string(),
            }
        } else {
            "unknown".to_string()
        };
        eprintln!(
            "get_effective_margin_start: node={:?} ({}), raw={} ({}px), collapsed={} ({}px), participates={}",
            scoped.node(),
            tag,
            raw_margin,
            raw_margin / 64,
            result,
            result / 64,
            participates
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

// ============================================================================
// Internal Implementation
// ============================================================================

/// Compute collapsed margin for the start edge (top) of a block element.
fn compute_collapsed_margin_start(scoped: &mut ScopedDb) -> Subpixels {
    let mut margins = vec![scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>()];

    // Collect margins that should collapse together
    collect_collapsing_margins_start(scoped, &mut margins);

    // Apply collapsing algorithm
    collapse_margins(&margins)
}

/// Compute collapsed margin for the end edge (bottom) of a block element.
fn compute_collapsed_margin_end(scoped: &mut ScopedDb) -> Subpixels {
    let mut margins = vec![scoped.query::<MarginQuery<rewrite_css::BlockMarker, EndMarker>>()];

    // Collect margins that should collapse together
    collect_collapsing_margins_end(scoped, &mut margins);

    // Apply collapsing algorithm
    collapse_margins(&margins)
}

/// Collect all margins that should collapse with the start margin.
fn collect_collapsing_margins_start(scoped: &mut ScopedDb, margins: &mut Vec<Subpixels>) {
    let node = scoped.node();

    // 1. Check previous sibling's bottom margin
    let prev_sibling = {
        let db = scoped.db();
        db.resolve_relationship(node, Relationship::PreviousSiblings)
            .last()
            .copied()
    };

    if let Some(prev_sibling) = prev_sibling {
        let adjoining = {
            let display = scoped.node_query::<DisplayQuery>(prev_sibling);
            let current_display = scoped.query::<DisplayQuery>();
            matches!(display, CssValue::Keyword(CssKeyword::Block))
                && matches!(current_display, CssValue::Keyword(CssKeyword::Block))
        };

        if adjoining {
            let mut sibling_scoped = scoped.scoped_to(prev_sibling);
            let prev_margin = get_effective_margin_end(&mut sibling_scoped);
            margins.push(prev_margin);
        }
    } else {
        // No previous sibling: check parent's top margin
        let can_collapse = can_collapse_with_parent_start(scoped);
        eprintln!(
            "collect_collapsing_margins_start: node={:?}, can_collapse_with_parent={}",
            node, can_collapse
        );

        if can_collapse {
            let parent = {
                let db = scoped.db();
                db.resolve_relationship(node, Relationship::Parent)
                    .first()
                    .copied()
            };

            if let Some(parent) = parent {
                let parent_margin =
                    scoped.node_query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>(parent);
                eprintln!(
                    "collect_collapsing_margins_start: adding parent margin {} ({}px)",
                    parent_margin,
                    parent_margin / 64
                );
                margins.push(parent_margin);
            }
        }
    }

    // 2. Check first child's top margin (if no separating border/padding)
    if can_collapse_with_first_child(scoped) {
        let first_child = get_first_in_flow_child(scoped);

        if let Some(first_child) = first_child {
            let mut child_scoped = scoped.scoped_to(first_child);
            let child_margin = get_effective_margin_start(&mut child_scoped);
            margins.push(child_margin);
        }
    }
}

/// Collect all margins that should collapse with the end margin.
fn collect_collapsing_margins_end(scoped: &mut ScopedDb, margins: &mut Vec<Subpixels>) {
    let node = scoped.node();

    // 1. Check last child's bottom margin
    if can_collapse_with_last_child(scoped) {
        let last_child = get_last_in_flow_child(scoped);

        if let Some(last_child) = last_child {
            let mut child_scoped = scoped.scoped_to(last_child);
            let child_margin = get_effective_margin_end(&mut child_scoped);
            margins.push(child_margin);
        }
    }

    // 2. Check if this is an empty block
    if is_empty_block(scoped) {
        let top_margin = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
        margins.push(top_margin);
    }
}

/// Apply the margin collapsing algorithm to a set of margins.
fn collapse_margins(margins: &[Subpixels]) -> Subpixels {
    if margins.is_empty() {
        return 0;
    }

    // Filter out MARGIN_AUTO sentinel values - auto margins don't participate in collapsing
    let valid_margins: Vec<Subpixels> = margins
        .iter()
        .copied()
        .filter(|&m| m != MARGIN_AUTO)
        .collect();

    if valid_margins.is_empty() {
        return 0;
    }

    let positive_margins: Vec<Subpixels> =
        valid_margins.iter().copied().filter(|&m| m > 0).collect();
    let negative_margins: Vec<Subpixels> =
        valid_margins.iter().copied().filter(|&m| m < 0).collect();

    let max_positive = positive_margins.iter().max().copied().unwrap_or(0);
    let min_negative = negative_margins.iter().min().copied().unwrap_or(0);

    max_positive + min_negative
}

/// Check if element's top margin can collapse with parent's top margin.
fn can_collapse_with_parent_start(scoped: &mut ScopedDb) -> bool {
    let Some(parent) = scoped.parent_id() else {
        return false;
    };

    // Check if parent is the document node - document doesn't participate in margin collapsing
    // but its children (like <html>) can still collapse margins with their children
    let parent_is_document = {
        let db = scoped.db();
        matches!(
            db.get_node_data::<rewrite_html::NodeData>(parent),
            Some(rewrite_html::NodeData::Document)
        )
    };
    if parent_is_document {
        // Can't collapse with document, but this is fine - just means no collapsing at root level
        return false;
    }

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

/// Check if element's top margin can collapse with its first child's top margin.
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

    // Check first in-flow child exists and is block
    if let Some(first_child) = get_first_in_flow_child(scoped) {
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

    // Check last in-flow child exists and is block
    if let Some(last_child) = get_last_in_flow_child(scoped) {
        is_in_flow_block(scoped, last_child)
    } else {
        false
    }
}

/// Check if this is an empty block whose top and bottom margins should collapse.
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

    // Check no content - simplified check
    // We can't query size here because it would create a circular dependency
    // Instead, just check if there are any children
    let has_children = scoped.first_child().is_some();
    !has_children
}

/// Check if an element participates in margin collapsing.
fn participates_in_margin_collapsing(scoped: &mut ScopedDb) -> bool {
    // Check not absolutely positioned
    let position = scoped.query::<PositionQuery>();
    if matches!(
        position,
        CssValue::Keyword(CssKeyword::Absolute) | CssValue::Keyword(CssKeyword::Fixed)
    ) {
        return false;
    }

    // Check is block-level
    let is_block = is_in_flow_block_current(scoped);
    if !is_block {
        let display = scoped.query::<DisplayQuery>();
        eprintln!(
            "participates_in_margin_collapsing: node={:?} not block-level, display={:?}",
            scoped.node(),
            display
        );
        return false;
    }

    // Check doesn't establish BFC
    let establishes_bfc = establishes_bfc_current(scoped);
    if establishes_bfc {
        eprintln!(
            "participates_in_margin_collapsing: node={:?} establishes BFC, not participating",
            scoped.node()
        );
        return false;
    }

    true
}

/// Get the first in-flow child of the current element.
/// Skips text nodes and display:none elements.
fn get_first_in_flow_child(scoped: &mut ScopedDb) -> Option<NodeId> {
    let node = scoped.node();
    let db = scoped.db();
    let children = db.resolve_relationship(node, Relationship::Children);

    for &child in &children {
        // Check if this child participates in layout
        let display = scoped.node_query::<DisplayQuery>(child);
        if matches!(display, CssValue::Keyword(CssKeyword::Block)) {
            return Some(child);
        }
    }
    None
}

/// Get the last in-flow child of the current element.
/// Skips text nodes and display:none elements.
fn get_last_in_flow_child(scoped: &mut ScopedDb) -> Option<NodeId> {
    let node = scoped.node();
    let db = scoped.db();
    let children = db.resolve_relationship(node, Relationship::Children);

    for &child in children.iter().rev() {
        // Check if this child participates in layout
        let display = scoped.node_query::<DisplayQuery>(child);
        if matches!(display, CssValue::Keyword(CssKeyword::Block)) {
            return Some(child);
        }
    }
    None
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
fn establishes_bfc_current(scoped: &mut ScopedDb) -> bool {
    rewrite_layout_bfc::establishes_bfc(scoped)
}
