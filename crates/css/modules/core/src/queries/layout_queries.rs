//! Core layout queries for constraint-based layout computation.

use crate::LayoutUnit;
use crate::box_tree::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace};
use crate::box_tree::exclusion_space::ExclusionSpace;
use crate::box_tree::margin_strut::MarginStrut;
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;
use std::sync::Arc;
use valor_query::{InputQuery, Query, QueryDatabase};

/// Input query for computed styles (provided by style computation phase).
pub struct ComputedStyleInput;

impl InputQuery for ComputedStyleInput {
    type Key = NodeKey;
    type Value = ComputedStyle;

    fn default_value() -> Self::Value {
        ComputedStyle::default()
    }
}

/// Input query for viewport dimensions.
pub struct ViewportInput;

impl InputQuery for ViewportInput {
    type Key = ();
    type Value = (LayoutUnit, LayoutUnit); // (width, height)

    fn default_value() -> Self::Value {
        (
            LayoutUnit::from_raw(800 * 64),
            LayoutUnit::from_raw(600 * 64),
        )
    }
}

/// Input query for storing a child's constraint space.
///
/// This is set by the parent during layout and queried by the child.
pub struct ConstraintSpaceInput;

impl InputQuery for ConstraintSpaceInput {
    type Key = NodeKey; // Just the child node
    type Value = ConstraintSpace;

    fn default_value() -> Self::Value {
        // Default to a root-like constraint space
        ConstraintSpace::new_for_root(
            LayoutUnit::from_raw(800 * 64),
            LayoutUnit::from_raw(600 * 64),
        )
    }
}

/// Result of layout computation for a single node.
#[derive(Debug, Clone)]
pub struct LayoutResult {
    /// Computed inline size (width in horizontal writing mode)
    pub inline_size: f32,

    /// Computed block size (height in horizontal writing mode)
    pub block_size: f32,

    /// Position in the block formatting context
    pub bfc_offset: BfcOffset,

    /// Updated exclusion space after this node (for floats)
    pub exclusion_space: Arc<ExclusionSpace>,

    /// Baseline position for alignment
    pub baseline: Option<f32>,

    /// Margin strut that collapsed through this element from the top
    /// Used to adjust parent positioning when margins collapse through
    pub collapsed_through_top_margin: LayoutUnit,

    /// Margin strut that collapsed through this element from the bottom
    /// Used to adjust parent height when margins collapse through
    pub collapsed_through_bottom_margin: LayoutUnit,
}

impl Default for LayoutResult {
    fn default() -> Self {
        Self {
            inline_size: 0.0,
            block_size: 0.0,
            bfc_offset: BfcOffset::root(),
            exclusion_space: Arc::new(ExclusionSpace::new()),
            baseline: None,
            collapsed_through_top_margin: LayoutUnit::zero(),
            collapsed_through_bottom_margin: LayoutUnit::zero(),
        }
    }
}

/// Query for the layout result of a node.
///
/// This computes the layout for a single node by:
/// 1. Getting the node's computed style
/// 2. Getting the constraint space from the parent
/// 3. Laying out children (via recursive queries)
/// 4. Computing this node's final size and position
///
/// This is truly incremental - changes to a node only recompute affected subtrees.
pub struct LayoutResultQuery;

impl Query for LayoutResultQuery {
    type Key = NodeKey;
    type Value = LayoutResult;

    fn execute(db: &QueryDatabase, key: Self::Key) -> Self::Value {
        let node = key;

        // Check if this is a text node
        use css_orchestrator::queries::DomTextInput;
        let text_content = db.input::<DomTextInput>(node);
        if text_content.is_some() {
            // This is a text node - measure it
            return layout_text_node(db, node, &text_content);
        }

        // Get computed style via query (not input)
        use css_orchestrator::queries::ComputedStyleQuery;
        let style = db.query::<ComputedStyleQuery>(node);

        // Check display: none
        if matches!(style.display, css_orchestrator::style_model::Display::None) {
            return LayoutResult::default();
        }

        // Get constraint space for this node
        let constraint_space = get_constraint_space_for_node(db, node);

        // Dispatch to appropriate layout algorithm based on display type
        let result = match style.display {
            css_orchestrator::style_model::Display::Flex
            | css_orchestrator::style_model::Display::InlineFlex => {
                layout_flex_container(db, node, &style, &constraint_space)
            }
            css_orchestrator::style_model::Display::Grid
            | css_orchestrator::style_model::Display::InlineGrid => {
                layout_grid_container(db, node, &style, &constraint_space)
            }
            _ => {
                // Block or inline - use block layout
                layout_block_node(db, node, &style, &constraint_space)
            }
        };

        log::trace!(
            "LayoutResultQuery: node={:?} -> inline={}, block={}, bfc_offset=({}, {:?})",
            node,
            result.inline_size,
            result.block_size,
            result.bfc_offset.inline_offset.to_px(),
            result.bfc_offset.block_offset.map(|b| b.to_px())
        );

        result
    }
}

// Use the DomParentInput from css_orchestrator to ensure we read the same data
pub use css_orchestrator::queries::dom_inputs::DomParentInput;

// Use the DomChildrenInput from css_orchestrator to ensure we read the same data
pub use css_orchestrator::queries::dom_inputs::DomChildrenInput;

/// Layout a text node by measuring its content.
fn layout_text_node(
    db: &QueryDatabase,
    node: NodeKey,
    text_content: &Arc<Option<String>>,
) -> LayoutResult {
    // Get the text node's own style (it inherits from parent automatically via cascade)
    use css_orchestrator::queries::ComputedStyleQuery;
    let style = db.query::<ComputedStyleQuery>(node);

    // Get constraint space
    let space = get_constraint_space_for_node(db, node);

    // Measure the text using css_text module
    let text = match text_content.as_ref() {
        Some(t) => t,
        None => return LayoutResult::default(),
    };

    // Whitespace-only text nodes in block formatting contexts should not contribute to layout
    // (they get collapsed during white-space processing)
    if text.trim().is_empty() {
        return LayoutResult {
            inline_size: 0.0,
            block_size: 0.0,
            bfc_offset: space.bfc_offset.clone(),
            exclusion_space: Arc::new(space.exclusion_space.clone()),
            baseline: None,
            collapsed_through_top_margin: LayoutUnit::zero(),
            collapsed_through_bottom_margin: LayoutUnit::zero(),
        };
    }

    // Use the text measurement system from css_text
    use css_text::measurement::measure_text;

    let metrics = measure_text(text, &style);

    LayoutResult {
        inline_size: metrics.width,
        block_size: metrics.height,
        bfc_offset: space.bfc_offset.clone(),
        exclusion_space: Arc::new(space.exclusion_space.clone()),
        baseline: Some(metrics.ascent),
        collapsed_through_top_margin: LayoutUnit::zero(),
        collapsed_through_bottom_margin: LayoutUnit::zero(),
    }
}

/// Get the constraint space for a node.
///
/// For the root node, creates a constraint space from the viewport.
/// For other nodes, retrieves the constraint space that was set by the parent.
fn get_constraint_space_for_node(db: &QueryDatabase, node: NodeKey) -> ConstraintSpace {
    let parent_opt = db.input::<DomParentInput>(node);

    match *parent_opt {
        Some(parent) if parent != NodeKey::ROOT => {
            // Get constraint space that was set by parent
            (*db.input::<ConstraintSpaceInput>(node)).clone()
        }
        _ => {
            // Root node - use viewport as initial containing block
            let viewport = db.input::<ViewportInput>(());
            let (icb_width, icb_height) = *viewport;
            ConstraintSpace::new_for_root(icb_width, icb_height)
        }
    }
}

/// Layout a block-level node.
///
/// This implements the CSS block layout algorithm for a single node.
/// Children are laid out via recursive queries.
fn layout_block_node(
    db: &QueryDatabase,
    node: NodeKey,
    style: &css_orchestrator::style_model::ComputedStyle,
    space: &ConstraintSpace,
) -> LayoutResult {
    use css_box::compute_box_sides;

    // Compute box model (margin, border, padding)
    let sides = compute_box_sides(style);

    // Compute inline size (width)
    let available_inline = space
        .available_inline_size
        .resolve(LayoutUnit::from_raw(800 * 64));

    let tag = db.input::<css_orchestrator::queries::DomTagInput>(node);
    log::trace!(
        "layout_block_node: tag={} node={:?} style.width={:?}, available={}px, space.bfc_offset.block_offset={:?}",
        tag,
        node,
        style.width,
        available_inline.to_px(),
        space.bfc_offset.block_offset.map(|b| b.to_px())
    );

    // Compute inline size with auto margin handling
    let (inline_size, actual_margin_left, actual_margin_right) = if let Some(width_px) = style.width
    {
        // Explicit width: handle auto margins for centering
        if style.margin_left_auto && style.margin_right_auto {
            // Both margins auto: center the block
            let border_padding_inline =
                (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                    .to_px();
            let remaining_space =
                (available_inline.to_px() - width_px - border_padding_inline).max(0.0);
            let auto_margin = remaining_space / 2.0;
            (
                width_px,
                LayoutUnit::from_px(auto_margin),
                LayoutUnit::from_px(auto_margin),
            )
        } else if style.margin_left_auto {
            // Left margin auto: push to the right
            let border_padding_inline =
                (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                    .to_px();
            let remaining_space = (available_inline.to_px()
                - width_px
                - sides.margin_right.to_px()
                - border_padding_inline)
                .max(0.0);
            (
                width_px,
                LayoutUnit::from_px(remaining_space),
                sides.margin_right,
            )
        } else if style.margin_right_auto {
            // Right margin auto: push to the left
            let border_padding_inline =
                (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                    .to_px();
            let remaining_space = (available_inline.to_px()
                - width_px
                - sides.margin_left.to_px()
                - border_padding_inline)
                .max(0.0);
            (
                width_px,
                sides.margin_left,
                LayoutUnit::from_px(remaining_space),
            )
        } else {
            // No auto margins
            (width_px, sides.margin_left, sides.margin_right)
        }
    } else {
        // Auto width = fill available space minus margins
        let margin_inline = (sides.margin_left + sides.margin_right).to_px();
        let border_padding_inline =
            (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                .to_px();
        let width = (available_inline.to_px() - margin_inline - border_padding_inline).max(0.0);
        (width, sides.margin_left, sides.margin_right)
    };

    // Get children
    let children_arc: Arc<Vec<NodeKey>> = db.input::<DomChildrenInput>(node);
    let children = &*children_arc;

    log::trace!(
        "layout_block_node: node={:?} has {} children",
        node,
        children.len()
    );

    // Layout children and accumulate block size with margin collapsing
    let mut current_block_offset = LayoutUnit::zero();
    let mut pending_margin_strut = space.margin_strut.clone();

    // Check if this element establishes a Block Formatting Context (BFC)
    // BFCs prevent margin collapsing with children
    let establishes_bfc = !matches!(
        style.overflow,
        css_orchestrator::style_model::Overflow::Visible
    ) || matches!(
        style.display,
        css_orchestrator::style_model::Display::Flex
            | css_orchestrator::style_model::Display::Grid
            | css_orchestrator::style_model::Display::InlineBlock
    ) || !matches!(style.float, css_orchestrator::style_model::Float::None);

    // Check if margins can collapse through this element
    // Margins cannot collapse through if there's padding, border, or if element establishes BFC
    let can_collapse_through_top =
        sides.border_top.to_px() == 0.0 && sides.padding_top.to_px() == 0.0 && !establishes_bfc;
    let can_collapse_through_bottom = sides.border_bottom.to_px() == 0.0
        && sides.padding_bottom.to_px() == 0.0
        && !establishes_bfc;

    let mut is_first_block_child = true;
    let mut collapsed_through_top_margin = LayoutUnit::zero();

    let content_inline_size = inline_size
        - (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
            .to_px();

    // Track exclusion space (updated as we add floats)
    let mut current_exclusion_space = space.exclusion_space.clone();

    for (child_index, &child) in children.iter().enumerate() {
        // Get child's computed style to check margins
        use css_orchestrator::queries::ComputedStyleQuery;
        let child_style = db.query::<ComputedStyleQuery>(child);

        // Get child's box sides for margins
        let child_sides = css_box::compute_box_sides(&child_style);

        // Check if this is a floated element
        let is_float = !matches!(
            child_style.float,
            css_orchestrator::style_model::Float::None
        );

        // Check if this is a block-level element (participates in margin collapsing)
        // Inline elements (text nodes, inline boxes) should NOT consume margin struts
        let is_block_level = matches!(
            child_style.display,
            css_orchestrator::style_model::Display::Block
                | css_orchestrator::style_model::Display::Flex
                | css_orchestrator::style_model::Display::Grid
                | css_orchestrator::style_model::Display::Table
        );

        // TODO: Handle floats specially - they don't participate in normal flow
        // Disabled for now to focus on other layout issues
        // if is_float {
        //     continue;
        // }

        // Only process margin strut for block-level elements (and non-floats)
        let collapsed_margin = if is_block_level && !is_float {
            // Append child's top margin to the pending margin strut
            pending_margin_strut.append(LayoutUnit::from_px(child_style.margin.top));

            // Collapse the margin strut and advance block offset
            pending_margin_strut.collapse()
        } else {
            // Inline element - don't consume the margin strut, just pass it through
            LayoutUnit::zero()
        };

        // Handle first block child specially for margin collapsing through parent
        if is_first_block_child && is_block_level && can_collapse_through_top {
            // First block child with collapsible top - don't advance offset yet
            // The margin will be returned to our parent
            collapsed_through_top_margin = collapsed_margin;
            // Note: Don't set is_first_block_child = false yet - we'll do it after
            // checking if this child is empty
            // Don't advance current_block_offset - the margin collapses through
        } else if is_block_level {
            // Regular block child - advance by collapsed margin
            current_block_offset = current_block_offset + collapsed_margin;
            is_first_block_child = false;
        }

        // Reset margin strut after collapsing for block elements only
        if is_block_level {
            pending_margin_strut = MarginStrut::default();
        }

        // Create constraint space for child
        // Children are positioned relative to parent's content box (padding edge)
        // Parent's margin is already included in parent's bfc_offset, so we don't add it again
        // For inline children (like text nodes), don't include child margins in positioning
        // Only block-level children have their margins affect their position
        let child_inline_offset = if is_block_level {
            space.bfc_offset.inline_offset
                + sides.padding_left
                + sides.border_left
                + child_sides.margin_left
        } else {
            // Inline children: position at parent's content box edge (no child margin)
            space.bfc_offset.inline_offset + sides.padding_left + sides.border_left
        };

        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                content_inline_size,
            )),
            available_block_size: space.available_block_size,
            bfc_offset: BfcOffset::new(
                child_inline_offset,
                Some(
                    space.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                        + current_block_offset
                        + sides.padding_top
                        + sides.border_top,
                ),
            ),
            exclusion_space: current_exclusion_space.clone(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: false,
            percentage_resolution_block_size: space.percentage_resolution_block_size,
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: false,
            margins_already_applied: false,
            is_block_size_forced: false,
            is_inline_size_forced: false,
        };

        // Store constraint space for this child
        db.set_input::<ConstraintSpaceInput>(child, child_space.clone());

        // Query child layout (recursive!)
        let child_result = db.query::<LayoutResultQuery>(child);

        log::trace!(
            "Child {:?} layout result: bfc_offset.block_offset={:?}",
            child,
            child_result.bfc_offset.block_offset.map(|b| b.to_px())
        );

        // If child has margins that collapsed through it, we need to handle it
        if child_result.collapsed_through_top_margin != LayoutUnit::zero() {
            // Check if this is the first block child and we can collapse through top
            let is_collapsing_first_child = is_block_level
                && collapsed_through_top_margin != LayoutUnit::zero()
                && can_collapse_through_top;

            if is_collapsing_first_child {
                // Re-calculate the collapsed margin: max(child's margin, child's collapsed-through)
                let child_margin = LayoutUnit::from_px(child_style.margin.top);
                let total_collapsed = child_margin.max(child_result.collapsed_through_top_margin);
                collapsed_through_top_margin = total_collapsed;
                // Don't advance offset - this margin escapes through us
            } else {
                // Not first child or can't collapse through - position child lower
                let adjusted_space = ConstraintSpace {
                    bfc_offset: BfcOffset::new(
                        child_space.bfc_offset.inline_offset,
                        child_space
                            .bfc_offset
                            .block_offset
                            .map(|offset| offset + child_result.collapsed_through_top_margin),
                    ),
                    ..child_space
                };
                db.set_input::<ConstraintSpaceInput>(child, adjusted_space);

                // Advance our current offset by this amount
                current_block_offset =
                    current_block_offset + child_result.collapsed_through_top_margin;
            }
        }

        // Advance block offset by child's content height
        current_block_offset = current_block_offset + LayoutUnit::from_px(child_result.block_size);

        // Handle child's bottom margin
        if is_block_level {
            // Check if this child was completely empty (margins collapsed through both top and bottom)
            let child_collapsed_through = child_result.collapsed_through_top_margin
                != LayoutUnit::zero()
                && child_result.collapsed_through_bottom_margin != LayoutUnit::zero()
                && child_result.block_size == 0.0;

            if child_collapsed_through {
                // Empty element: margins collapsed through both top and bottom
                // The combined margin should affect the next sibling
                // Add it to the pending margin strut
                pending_margin_strut.append(child_result.collapsed_through_bottom_margin);

                // If this was the first child that collapsed through the parent,
                // keep is_first_block_child as true so the next sibling also doesn't advance offset
                // (This handles the case where multiple empty elements or an empty + non-empty
                // sibling share the same collapsed top margin space)
                // is_first_block_child stays as it was
            } else {
                // Non-empty element - next sibling will be a regular child
                if is_first_block_child {
                    is_first_block_child = false;
                }

                if child_result.collapsed_through_bottom_margin != LayoutUnit::zero() {
                    // Child has bottom margin that collapsed through (but not top)
                    // This can happen with block formatting contexts
                    pending_margin_strut.append(child_result.collapsed_through_bottom_margin);
                } else {
                    // Normal case: add child's bottom margin
                    pending_margin_strut.append(LayoutUnit::from_px(child_style.margin.bottom));
                }
            }
        }
    }

    // Handle bottom margin collapsing through parent
    // Bottom margins collapse through parent when parent has no bottom padding/border
    let can_collapse_bottom_through_parent =
        sides.border_bottom.to_px() == 0.0 && sides.padding_bottom.to_px() == 0.0;

    let final_collapsed_margin = pending_margin_strut.collapse();

    // Only include bottom margin in height if it can't collapse through parent
    if !can_collapse_bottom_through_parent {
        current_block_offset = current_block_offset + final_collapsed_margin;
    }

    // Compute final block size
    let block_size = if let Some(height_px) = style.height {
        height_px
    } else {
        // Auto height = sum of children + padding + border
        let children_height = current_block_offset.to_px();
        let vertical_spacing =
            (sides.padding_top + sides.padding_bottom + sides.border_top + sides.border_bottom)
                .to_px();

        // Note: Empty elements with no explicit height will have height = 0
        // This is correct per CSS spec and allows margin collapsing through them
        children_height + vertical_spacing
    };

    // Check if this element is empty and both margins collapse through
    let is_empty_and_collapses_through = block_size == 0.0
        && can_collapse_through_top
        && can_collapse_through_bottom
        && current_block_offset.to_px() == 0.0;

    // Return the margins that collapsed through this element
    let (returned_top_margin, returned_bottom_margin) = if is_empty_and_collapses_through {
        // Empty element: top and bottom margins collapse together into a single margin
        // Combine element's own top margin, bottom margin, and any collapsed margins from children
        let mut combined_strut = MarginStrut::default();
        combined_strut.append(LayoutUnit::from_px(style.margin.top));
        combined_strut.append(LayoutUnit::from_px(style.margin.bottom));
        combined_strut.append(collapsed_through_top_margin);
        combined_strut.append(final_collapsed_margin);
        let combined_margin = combined_strut.collapse();

        // Return the same combined margin for both top and bottom
        // The parent will use whichever is appropriate
        (combined_margin, combined_margin)
    } else {
        // Non-empty element: return margins normally
        let top = if can_collapse_through_top {
            collapsed_through_top_margin
        } else {
            LayoutUnit::zero()
        };

        let bottom = if can_collapse_through_bottom {
            final_collapsed_margin
        } else {
            LayoutUnit::zero()
        };

        (top, bottom)
    };

    LayoutResult {
        inline_size,
        block_size,
        bfc_offset: space.bfc_offset.clone(),
        exclusion_space: Arc::new(current_exclusion_space),
        baseline: None,
        collapsed_through_top_margin: returned_top_margin,
        collapsed_through_bottom_margin: returned_bottom_margin,
    }
}

/// Layout a flex container using the css_flexbox module.
fn layout_flex_container(
    db: &QueryDatabase,
    node: NodeKey,
    style: &css_orchestrator::style_model::ComputedStyle,
    space: &ConstraintSpace,
) -> LayoutResult {
    use css_box::compute_box_sides;
    use css_flexbox::{
        CrossAndBaseline, CrossContext, FlexChild, FlexContainerInputs, ItemRef, WritingMode,
        layout_multi_line_with_cross, layout_single_line_with_cross,
    };

    let sides = compute_box_sides(style);

    // Compute container size
    let available_inline = space
        .available_inline_size
        .resolve(LayoutUnit::from_raw(800 * 64));
    let inline_size = if let Some(width_px) = style.width {
        width_px
    } else {
        let margin_inline = (sides.margin_left + sides.margin_right).to_px();
        let border_padding_inline =
            (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                .to_px();
        (available_inline.to_px() - margin_inline - border_padding_inline).max(0.0)
    };

    let content_inline_size = inline_size
        - (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
            .to_px();

    // Get children
    let children_arc: Arc<Vec<NodeKey>> = db.input::<DomChildrenInput>(node);
    let children = &*children_arc;

    // Determine main axis size based on flex direction
    let is_row = matches!(
        style.flex_direction,
        css_orchestrator::style_model::FlexDirection::Row
    );

    let container_main_size = if is_row {
        content_inline_size
    } else {
        // For column, use available height or explicit height
        if let Some(height_px) = style.height {
            height_px
                - (sides.padding_top
                    + sides.padding_bottom
                    + sides.border_top
                    + sides.border_bottom)
                    .to_px()
        } else {
            space
                .available_block_size
                .resolve(LayoutUnit::from_raw(600 * 64))
                .to_px()
                - (sides.padding_top
                    + sides.padding_bottom
                    + sides.border_top
                    + sides.border_bottom)
                    .to_px()
        }
    };

    // Convert children to flex items and measure them
    let mut flex_children = Vec::new();
    let mut cross_inputs = Vec::new();
    let mut baseline_inputs = Vec::new();

    for (idx, &child) in children.iter().enumerate() {
        // Get child's computed style
        use css_orchestrator::queries::ComputedStyleQuery;
        let child_style = db.query::<ComputedStyleQuery>(child);

        // Skip display:none children
        if matches!(
            child_style.display,
            css_orchestrator::style_model::Display::None
        ) {
            continue;
        }

        // Measure child in flex base size constraint
        let measure_space =
            create_flex_measurement_space(&child_style, &sides, space, is_row, content_inline_size);
        db.set_input::<ConstraintSpaceInput>(child, measure_space);
        let child_measure = db.query::<LayoutResultQuery>(child);

        let child_sides = compute_box_sides(&child_style);

        // Determine flex basis
        let flex_basis = if let Some(basis) = child_style.flex_basis {
            basis
        } else if let Some(size) = if is_row {
            child_style.width
        } else {
            child_style.height
        } {
            size
        } else {
            // Auto flex-basis: use content size
            if is_row {
                child_measure.inline_size
            } else {
                child_measure.block_size
            }
        };

        // Main axis padding + border
        let main_padding_border = if is_row {
            (child_sides.padding_left
                + child_sides.padding_right
                + child_sides.border_left
                + child_sides.border_right)
                .to_px()
        } else {
            (child_sides.padding_top
                + child_sides.padding_bottom
                + child_sides.border_top
                + child_sides.border_bottom)
                .to_px()
        };

        // Create flex child
        // Determine min/max constraints based on flex direction
        let (min_main, max_main) = if is_row {
            (
                child_style.min_width.unwrap_or(0.0),
                child_style.max_width.unwrap_or(f32::INFINITY),
            )
        } else {
            (
                child_style.min_height.unwrap_or(0.0),
                child_style.max_height.unwrap_or(f32::INFINITY),
            )
        };

        let flex_child = FlexChild {
            handle: ItemRef(idx as u64),
            flex_basis,
            flex_grow: child_style.flex_grow,
            flex_shrink: child_style.flex_shrink,
            min_main,
            max_main,
            margin_left: child_sides.margin_left.to_px(),
            margin_right: child_sides.margin_right.to_px(),
            margin_top: child_sides.margin_top.to_px(),
            margin_bottom: child_sides.margin_bottom.to_px(),
            margin_left_auto: child_style.margin_left_auto,
            margin_right_auto: child_style.margin_right_auto,
            main_padding_border,
        };
        flex_children.push(flex_child);

        // Cross size for this child
        let cross_size = if is_row {
            child_measure.block_size
        } else {
            child_measure.inline_size
        };
        cross_inputs.push((
            css_flexbox::CrossSize::Explicit(cross_size),
            0.0,           // min_cross
            f32::INFINITY, // max_cross
        ));

        // Baseline (using measured baseline or default)
        let first_baseline = child_measure.baseline.unwrap_or(cross_size);
        baseline_inputs.push(Some((first_baseline, first_baseline)));
    }

    // Set up flex container inputs
    let container = FlexContainerInputs {
        direction: convert_flex_direction(style.flex_direction),
        writing_mode: convert_writing_mode(style.writing_mode),
        container_main_size,
        main_gap: if is_row {
            style.column_gap
        } else {
            style.row_gap
        },
    };

    let justify_content = convert_justify_content_to_flex(style.justify_content);
    let align_items = convert_align_items_to_flex(style.align_items);
    let align_content = convert_align_content_to_flex(style.align_content);

    // Determine cross size
    let container_cross_size = if is_row {
        if let Some(height_px) = style.height {
            height_px
                - (sides.padding_top
                    + sides.padding_bottom
                    + sides.border_top
                    + sides.border_bottom)
                    .to_px()
        } else {
            // Auto height: sum of cross sizes
            cross_inputs
                .iter()
                .map(|(cs, _, _)| match cs {
                    css_flexbox::CrossSize::Explicit(s) | css_flexbox::CrossSize::Stretch(s) => *s,
                })
                .fold(0.0f32, f32::max)
        }
    } else {
        content_inline_size
    };

    let cross_ctx = CrossContext {
        align_items,
        align_content,
        container_cross_size,
        cross_gap: if is_row {
            style.row_gap
        } else {
            style.column_gap
        },
    };

    // Run flex layout algorithm
    let placements = if matches!(
        style.flex_wrap,
        css_orchestrator::style_model::FlexWrap::NoWrap
    ) {
        // Single-line flex layout
        let cab = CrossAndBaseline {
            cross_inputs: &cross_inputs,
            baseline_inputs: &baseline_inputs,
        };
        layout_single_line_with_cross(container, justify_content, cross_ctx, &flex_children, cab)
    } else {
        // Multi-line flex layout
        let cab = CrossAndBaseline {
            cross_inputs: &cross_inputs,
            baseline_inputs: &baseline_inputs,
        };
        layout_multi_line_with_cross(container, justify_content, cross_ctx, &flex_children, cab)
    };

    // Position children using flex results
    for (placement_idx, (main_placement, cross_placement)) in placements.iter().enumerate() {
        let child_idx = main_placement.handle.0 as usize;
        if child_idx >= children.len() {
            continue;
        }
        let child = children[child_idx];

        // Create constraint space for child at its flex position
        let (child_inline_offset, child_block_offset, child_inline_size, child_block_size) =
            if is_row {
                (
                    main_placement.main_offset,
                    cross_placement.cross_offset,
                    main_placement.main_size,
                    cross_placement.cross_size,
                )
            } else {
                (
                    cross_placement.cross_offset,
                    main_placement.main_offset,
                    cross_placement.cross_size,
                    main_placement.main_size,
                )
            };

        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(child_inline_size)),
            available_block_size: AvailableSize::Definite(LayoutUnit::from_px(child_block_size)),
            bfc_offset: BfcOffset::new(
                space.bfc_offset.inline_offset
                    + sides.padding_left
                    + sides.border_left
                    + LayoutUnit::from_px(child_inline_offset),
                Some(
                    space.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                        + sides.padding_top
                        + sides.border_top
                        + LayoutUnit::from_px(child_block_offset),
                ),
            ),
            exclusion_space: space.exclusion_space.clone(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: Some(LayoutUnit::from_px(child_block_size)),
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: false,
            margins_already_applied: false,
            is_block_size_forced: false,
            is_inline_size_forced: false,
        };

        db.set_input::<ConstraintSpaceInput>(child, child_space);

        // Trigger child layout
        let _child_result = db.query::<LayoutResultQuery>(child);
    }

    // Compute final container size
    let block_size = if let Some(height_px) = style.height {
        height_px
    } else {
        let content_size = if is_row {
            container_cross_size
        } else {
            // For column, use the maximum main offset + size
            placements
                .iter()
                .map(|(main, _)| main.main_offset + main.main_size)
                .fold(0.0f32, f32::max)
        };
        let vertical_spacing =
            (sides.padding_top + sides.padding_bottom + sides.border_top + sides.border_bottom)
                .to_px();
        content_size + vertical_spacing
    };

    LayoutResult {
        inline_size,
        block_size,
        bfc_offset: space.bfc_offset.clone(),
        exclusion_space: Arc::new(space.exclusion_space.clone()),
        baseline: None,
        collapsed_through_top_margin: LayoutUnit::zero(),
        collapsed_through_bottom_margin: LayoutUnit::zero(),
    }
}

/// Create a constraint space for measuring a flex item.
fn create_flex_measurement_space(
    child_style: &css_orchestrator::style_model::ComputedStyle,
    parent_sides: &css_box::BoxSides,
    parent_space: &ConstraintSpace,
    is_row: bool,
    content_inline_size: f32,
) -> ConstraintSpace {
    let available_inline = if is_row {
        AvailableSize::MaxContent
    } else {
        AvailableSize::Definite(LayoutUnit::from_px(content_inline_size))
    };

    let available_block = if is_row {
        if let Some(height) = child_style.height {
            AvailableSize::Definite(LayoutUnit::from_px(height))
        } else {
            AvailableSize::Indefinite
        }
    } else {
        AvailableSize::MaxContent
    };

    ConstraintSpace {
        available_inline_size: available_inline,
        available_block_size: available_block,
        bfc_offset: BfcOffset::new(
            parent_space.bfc_offset.inline_offset
                + parent_sides.padding_left
                + parent_sides.border_left,
            Some(
                parent_space
                    .bfc_offset
                    .block_offset
                    .unwrap_or(LayoutUnit::zero())
                    + parent_sides.padding_top
                    + parent_sides.border_top,
            ),
        ),
        exclusion_space: parent_space.exclusion_space.clone(),
        margin_strut: MarginStrut::default(),
        is_new_formatting_context: true,
        percentage_resolution_block_size: parent_space.percentage_resolution_block_size,
        fragmentainer_block_size: None,
        fragmentainer_offset: LayoutUnit::zero(),
        is_for_measurement_only: true,
        margins_already_applied: false,
        is_block_size_forced: false,
        is_inline_size_forced: false,
    }
}

/// Convert style model FlexDirection to css_flexbox FlexDirection.
fn convert_flex_direction(
    dir: css_orchestrator::style_model::FlexDirection,
) -> css_flexbox::FlexDirection {
    use css_flexbox::FlexDirection;
    use css_orchestrator::style_model::FlexDirection as StyleDir;

    match dir {
        StyleDir::Row => FlexDirection::Row,
        StyleDir::Column => FlexDirection::Column,
    }
}

/// Convert style model WritingMode to css_flexbox WritingMode.
fn convert_writing_mode(
    mode: css_orchestrator::style_model::WritingMode,
) -> css_flexbox::WritingMode {
    use css_flexbox::WritingMode;
    use css_orchestrator::style_model::WritingMode as StyleMode;

    match mode {
        StyleMode::HorizontalTb => WritingMode::HorizontalTb,
        StyleMode::VerticalRl => WritingMode::VerticalRl,
        StyleMode::VerticalLr => WritingMode::VerticalLr,
    }
}

/// Convert style model JustifyContent to css_flexbox JustifyContent.
fn convert_justify_content_to_flex(
    justify: css_orchestrator::style_model::JustifyContent,
) -> css_flexbox::JustifyContent {
    use css_flexbox::JustifyContent;
    use css_orchestrator::style_model::JustifyContent as StyleJustify;

    match justify {
        StyleJustify::FlexStart => JustifyContent::Start,
        StyleJustify::FlexEnd => JustifyContent::End,
        StyleJustify::Center => JustifyContent::Center,
        StyleJustify::SpaceBetween => JustifyContent::SpaceBetween,
        StyleJustify::SpaceAround => JustifyContent::SpaceAround,
        StyleJustify::SpaceEvenly => JustifyContent::SpaceEvenly,
    }
}

/// Convert style model AlignItems to css_flexbox AlignItems.
fn convert_align_items_to_flex(
    align: css_orchestrator::style_model::AlignItems,
) -> css_flexbox::AlignItems {
    use css_flexbox::AlignItems;
    use css_orchestrator::style_model::AlignItems as StyleAlign;

    match align {
        StyleAlign::Stretch => AlignItems::Stretch,
        StyleAlign::FlexStart => AlignItems::FlexStart,
        StyleAlign::FlexEnd => AlignItems::FlexEnd,
        StyleAlign::Center => AlignItems::Center,
        StyleAlign::Normal => AlignItems::Stretch,
    }
}

/// Convert style model AlignContent to css_flexbox AlignContent.
fn convert_align_content_to_flex(
    align: css_orchestrator::style_model::AlignContent,
) -> css_flexbox::AlignContent {
    use css_flexbox::AlignContent;
    use css_orchestrator::style_model::AlignContent as StyleAlign;

    match align {
        StyleAlign::FlexStart => AlignContent::Start,
        StyleAlign::FlexEnd => AlignContent::End,
        StyleAlign::Center => AlignContent::Center,
        StyleAlign::SpaceBetween => AlignContent::SpaceBetween,
        StyleAlign::SpaceAround => AlignContent::SpaceAround,
        StyleAlign::SpaceEvenly => AlignContent::SpaceEvenly,
        StyleAlign::Stretch => AlignContent::Stretch,
    }
}

/// Layout a grid container using the css_grid module.
fn layout_grid_container(
    db: &QueryDatabase,
    node: NodeKey,
    style: &css_orchestrator::style_model::ComputedStyle,
    space: &ConstraintSpace,
) -> LayoutResult {
    use css_box::compute_box_sides;
    use css_grid::{GridContainerInputs, GridItem, layout_grid};

    let sides = compute_box_sides(style);

    // Compute container size
    let available_inline = space
        .available_inline_size
        .resolve(LayoutUnit::from_raw(800 * 64));
    let inline_size = if let Some(width_px) = style.width {
        width_px
    } else {
        let margin_inline = (sides.margin_left + sides.margin_right).to_px();
        let border_padding_inline =
            (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                .to_px();
        (available_inline.to_px() - margin_inline - border_padding_inline).max(0.0)
    };

    let content_inline_size = inline_size
        - (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
            .to_px();

    // Get children and convert to GridItems
    let children_arc: Arc<Vec<NodeKey>> = db.input::<DomChildrenInput>(node);
    let children = &*children_arc;

    let grid_items: Vec<GridItem<NodeKey>> = children
        .iter()
        .map(|&child_key| GridItem::new(child_key))
        .collect();

    // Parse grid template from style (MVP: use default tracks if not specified)
    let (row_tracks, col_tracks) = parse_grid_templates(style);

    // Determine if we have an explicit height
    let has_explicit_height = style.height.is_some();

    // Get available block size
    let available_block_size = if let Some(height_px) = style.height {
        height_px
    } else {
        space
            .available_block_size
            .resolve(LayoutUnit::from_raw(600 * 64))
            .to_px()
    };

    let content_block_size = if let Some(height_px) = style.height {
        height_px
            - (sides.padding_top + sides.padding_bottom + sides.border_top + sides.border_bottom)
                .to_px()
    } else {
        available_block_size
            - (sides.padding_top + sides.padding_bottom + sides.border_top + sides.border_bottom)
                .to_px()
    };

    // Convert align/justify properties
    let align_items = convert_align_items_to_grid(style.align_items);
    let justify_items = convert_justify_content_to_grid(style.justify_content);

    let grid_inputs = GridContainerInputs {
        row_tracks,
        col_tracks,
        auto_flow: convert_grid_auto_flow(style.grid_auto_flow),
        available_width: content_inline_size,
        available_height: content_block_size,
        align_items,
        justify_items,
        has_explicit_height,
    };

    // Run grid layout algorithm
    let grid_result = match layout_grid(&grid_items, &grid_inputs) {
        Ok(result) => result,
        Err(e) => {
            log::warn!("Grid layout failed for node {:?}: {}", node, e);
            // Fall back to block layout on error
            return layout_block_node(db, node, style, space);
        }
    };

    // Position children using grid results
    for placed_item in &grid_result.items {
        let child = placed_item.node_id;

        // Create constraint space for child at its grid position
        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(placed_item.width)),
            available_block_size: AvailableSize::Definite(LayoutUnit::from_px(placed_item.height)),
            bfc_offset: BfcOffset::new(
                space.bfc_offset.inline_offset
                    + sides.padding_left
                    + sides.border_left
                    + LayoutUnit::from_px(placed_item.x),
                Some(
                    space.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                        + sides.padding_top
                        + sides.border_top
                        + LayoutUnit::from_px(placed_item.y),
                ),
            ),
            exclusion_space: space.exclusion_space.clone(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: Some(LayoutUnit::from_px(placed_item.height)),
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: false,
            margins_already_applied: false,
            is_block_size_forced: false,
            is_inline_size_forced: false,
        };

        db.set_input::<ConstraintSpaceInput>(child, child_space);

        // Trigger child layout
        let _child_result = db.query::<LayoutResultQuery>(child);
    }

    // Compute final container size
    let block_size = if let Some(height_px) = style.height {
        height_px
    } else {
        let vertical_spacing =
            (sides.padding_top + sides.padding_bottom + sides.border_top + sides.border_bottom)
                .to_px();
        grid_result.total_height + vertical_spacing
    };

    LayoutResult {
        inline_size,
        block_size,
        bfc_offset: space.bfc_offset.clone(),
        exclusion_space: Arc::new(space.exclusion_space.clone()),
        baseline: None,
        collapsed_through_top_margin: LayoutUnit::zero(),
        collapsed_through_bottom_margin: LayoutUnit::zero(),
    }
}

/// Parse grid template columns and rows from style.
fn parse_grid_templates(
    style: &css_orchestrator::style_model::ComputedStyle,
) -> (css_grid::GridAxisTracks, css_grid::GridAxisTracks) {
    use crate::box_tree::grid_template_parser::parse_grid_template;
    use css_grid::{GridAxisTracks, GridTrack, GridTrackSize, TrackBreadth, TrackListType};

    // Parse row tracks
    let row_tracks = if let Some(ref template) = style.grid_template_rows {
        parse_grid_template(template, style.row_gap)
    } else {
        // Default: single auto track
        GridAxisTracks::new(
            vec![GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Auto),
                track_type: TrackListType::Explicit,
            }],
            style.row_gap,
        )
    };

    // Parse column tracks
    let col_tracks = if let Some(ref template) = style.grid_template_columns {
        parse_grid_template(template, style.column_gap)
    } else {
        // Default: single auto track
        GridAxisTracks::new(
            vec![GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Auto),
                track_type: TrackListType::Explicit,
            }],
            style.column_gap,
        )
    };

    (row_tracks, col_tracks)
}

/// Convert style model AlignItems to grid alignment.
fn convert_align_items_to_grid(
    align: css_orchestrator::style_model::AlignItems,
) -> css_grid::GridAlignment {
    use css_grid::GridAlignment;
    use css_orchestrator::style_model::AlignItems;

    match align {
        AlignItems::FlexStart => GridAlignment::Start,
        AlignItems::FlexEnd => GridAlignment::End,
        AlignItems::Center => GridAlignment::Center,
        AlignItems::Stretch => GridAlignment::Stretch,
        AlignItems::Normal => GridAlignment::Stretch,
    }
}

/// Convert style model JustifyContent to grid alignment.
fn convert_justify_content_to_grid(
    justify: css_orchestrator::style_model::JustifyContent,
) -> css_grid::GridAlignment {
    use css_grid::GridAlignment;
    use css_orchestrator::style_model::JustifyContent;

    match justify {
        JustifyContent::FlexStart => GridAlignment::Start,
        JustifyContent::FlexEnd => GridAlignment::End,
        JustifyContent::Center => GridAlignment::Center,
        JustifyContent::SpaceBetween => GridAlignment::Stretch, // No direct equivalent
        JustifyContent::SpaceAround => GridAlignment::Stretch,
        JustifyContent::SpaceEvenly => GridAlignment::Stretch,
    }
}

/// Convert style model GridAutoFlow to css_grid GridAutoFlow.
fn convert_grid_auto_flow(
    flow: css_orchestrator::style_model::GridAutoFlow,
) -> css_grid::GridAutoFlow {
    use css_grid::GridAutoFlow;
    use css_orchestrator::style_model::GridAutoFlow as StyleFlow;

    match flow {
        StyleFlow::Row => GridAutoFlow::Row,
        StyleFlow::Column => GridAutoFlow::Column,
        StyleFlow::RowDense => GridAutoFlow::RowDense,
        StyleFlow::ColumnDense => GridAutoFlow::ColumnDense,
    }
}
