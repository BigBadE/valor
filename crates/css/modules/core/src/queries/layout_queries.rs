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
}

impl Default for LayoutResult {
    fn default() -> Self {
        Self {
            inline_size: 0.0,
            block_size: 0.0,
            bfc_offset: BfcOffset::root(),
            exclusion_space: Arc::new(ExclusionSpace::new()),
            baseline: None,
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

/// Input query for DOM parent relationship.
pub struct DomParentInput;

impl InputQuery for DomParentInput {
    type Key = NodeKey;
    type Value = Option<NodeKey>;

    fn default_value() -> Self::Value {
        None
    }
}

/// Input query for DOM children.
pub struct DomChildrenInput;

impl InputQuery for DomChildrenInput {
    type Key = NodeKey;
    type Value = Vec<NodeKey>;

    fn default_value() -> Self::Value {
        Vec::new()
    }
}

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

    let inline_size = if let Some(width_px) = style.width {
        width_px
    } else {
        // Auto width = fill available space minus margins
        let margin_inline = (sides.margin_left + sides.margin_right).to_px();
        let border_padding_inline =
            (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                .to_px();
        (available_inline.to_px() - margin_inline - border_padding_inline).max(0.0)
    };

    // Get children
    let children_arc: Arc<Vec<NodeKey>> = db.input::<DomChildrenInput>(node);
    let children = &*children_arc;

    log::trace!(
        "layout_block_node: node={:?} has {} children",
        node,
        children.len()
    );

    // Layout children and accumulate block size
    let mut current_block_offset = LayoutUnit::zero();
    let content_inline_size = inline_size
        - (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
            .to_px();

    for &child in children.iter() {
        // Create constraint space for child
        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                content_inline_size,
            )),
            available_block_size: space.available_block_size,
            bfc_offset: BfcOffset::new(
                space.bfc_offset.inline_offset + sides.padding_left + sides.border_left,
                Some(
                    space.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                        + current_block_offset
                        + sides.padding_top
                        + sides.border_top,
                ),
            ),
            exclusion_space: space.exclusion_space.clone(),
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
        db.set_input::<ConstraintSpaceInput>(child, child_space);

        // Query child layout (recursive!)
        let child_result = db.query::<LayoutResultQuery>(child);

        // Advance block offset
        current_block_offset = current_block_offset + LayoutUnit::from_px(child_result.block_size);
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
        let computed_height = children_height + vertical_spacing;

        // TEMPORARY FIX: If height is zero and we have no explicit height,
        // use a minimum based on font size to prevent collapsed elements
        if computed_height == 0.0 && children.is_empty() {
            // Use line-height as minimum for empty elements
            style.font_size * 1.2 // Default line-height multiplier
        } else {
            computed_height
        }
    };

    LayoutResult {
        inline_size,
        block_size,
        bfc_offset: space.bfc_offset.clone(),
        exclusion_space: Arc::new(space.exclusion_space.clone()),
        baseline: None,
    }
}

/// Layout a flex container.
///
/// This is a simplified implementation that handles basic flex layout.
/// Full implementation would use css_flexbox crate for complete spec compliance.
fn layout_flex_container(
    db: &QueryDatabase,
    node: NodeKey,
    style: &css_orchestrator::style_model::ComputedStyle,
    space: &ConstraintSpace,
) -> LayoutResult {
    use css_box::compute_box_sides;
    use css_orchestrator::style_model::FlexDirection;

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

    // Determine flex direction
    let is_row = matches!(style.flex_direction, FlexDirection::Row);

    // Get children
    let children_arc: Arc<Vec<NodeKey>> = db.input::<DomChildrenInput>(node);
    let children = &*children_arc;

    // Layout children and measure them
    let content_inline_size = inline_size
        - (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
            .to_px();

    let mut current_offset = LayoutUnit::zero();
    let mut max_cross_size = 0.0f32;

    for &child in children.iter() {
        // Create constraint space for child
        let child_space = if is_row {
            // Row flex: children laid out horizontally
            ConstraintSpace {
                available_inline_size: AvailableSize::MaxContent,
                available_block_size: space.available_block_size,
                bfc_offset: BfcOffset::new(
                    space.bfc_offset.inline_offset
                        + sides.padding_left
                        + sides.border_left
                        + current_offset,
                    Some(
                        space.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                            + sides.padding_top
                            + sides.border_top,
                    ),
                ),
                exclusion_space: space.exclusion_space.clone(),
                margin_strut: MarginStrut::default(),
                is_new_formatting_context: true,
                percentage_resolution_block_size: space.percentage_resolution_block_size,
                fragmentainer_block_size: None,
                fragmentainer_offset: LayoutUnit::zero(),
                is_for_measurement_only: false,
                margins_already_applied: false,
                is_block_size_forced: false,
                is_inline_size_forced: false,
            }
        } else {
            // Column flex: children laid out vertically
            ConstraintSpace {
                available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                    content_inline_size,
                )),
                available_block_size: AvailableSize::Indefinite,
                bfc_offset: BfcOffset::new(
                    space.bfc_offset.inline_offset + sides.padding_left + sides.border_left,
                    Some(
                        space.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                            + sides.padding_top
                            + sides.border_top
                            + current_offset,
                    ),
                ),
                exclusion_space: space.exclusion_space.clone(),
                margin_strut: MarginStrut::default(),
                is_new_formatting_context: true,
                percentage_resolution_block_size: None,
                fragmentainer_block_size: None,
                fragmentainer_offset: LayoutUnit::zero(),
                is_for_measurement_only: false,
                margins_already_applied: false,
                is_block_size_forced: false,
                is_inline_size_forced: false,
            }
        };

        db.set_input::<ConstraintSpaceInput>(child, child_space);

        // Query child layout
        let child_result = db.query::<LayoutResultQuery>(child);

        // Accumulate sizes
        if is_row {
            current_offset =
                current_offset + LayoutUnit::from_px(child_result.inline_size + style.column_gap);
            max_cross_size = max_cross_size.max(child_result.block_size);
        } else {
            current_offset =
                current_offset + LayoutUnit::from_px(child_result.block_size + style.row_gap);
            max_cross_size = max_cross_size.max(child_result.inline_size);
        }
    }

    // Compute final container size
    let block_size = if let Some(height_px) = style.height {
        height_px
    } else {
        let content_size = if is_row {
            max_cross_size
        } else {
            current_offset.to_px()
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
    }
}

/// Layout a grid container (placeholder).
fn layout_grid_container(
    _db: &QueryDatabase,
    _node: NodeKey,
    _style: &css_orchestrator::style_model::ComputedStyle,
    space: &ConstraintSpace,
) -> LayoutResult {
    // TODO: Implement grid layout properly
    LayoutResult {
        inline_size: 100.0,
        block_size: 100.0,
        bfc_offset: space.bfc_offset.clone(),
        exclusion_space: Arc::new(space.exclusion_space.clone()),
        baseline: None,
    }
}
