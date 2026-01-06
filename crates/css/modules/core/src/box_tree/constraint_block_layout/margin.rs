//! Margin collapsing logic for block layout.

use super::ConstraintLayoutTree;
use super::shared::EndMarginStrutParams;
use css_box::{BoxSides, LayoutUnit};
use css_orchestrator::style_model::ComputedStyle;

use super::super::constraint_space::{BfcOffset, ConstraintSpace, LayoutResult};
use super::super::margin_strut::MarginStrut;

impl ConstraintLayoutTree {
    pub(super) fn resolve_bfc_offset(
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
        establishes_bfc: bool,
    ) -> (BfcOffset, bool) {
        // Inline offset always includes left margin (margins don't collapse in inline direction)
        // UNLESS margins have already been applied by the parent layout algorithm (flex/grid)
        let inline_offset = if constraint_space.margins_already_applied {
            constraint_space.bfc_offset.inline_offset
        } else {
            constraint_space.bfc_offset.inline_offset + sides.margin_left
        };

        // If we establish a new BFC, we don't participate in parent's margin collapsing
        // Resolve any accumulated margins and add our own margin
        if establishes_bfc {
            let base_offset = constraint_space
                .bfc_offset
                .block_offset
                .unwrap_or(LayoutUnit::zero());
            let block_offset = if constraint_space.margins_already_applied {
                // Margins already in bfc_offset, don't add again
                Some(base_offset + constraint_space.margin_strut.collapse())
            } else {
                Some(base_offset + constraint_space.margin_strut.collapse() + sides.margin_top)
            };

            return (BfcOffset::new(inline_offset, block_offset), false);
        }

        // Check for clearance
        let clearance_floor = constraint_space
            .exclusion_space
            .clearance_offset(style.clear);

        // If clearance is needed, resolve margins and compute clearance
        if clearance_floor > LayoutUnit::zero() {
            // Clearance prevents margin collapsing, so we resolve accumulated margins
            let base_offset = constraint_space
                .bfc_offset
                .block_offset
                .unwrap_or(LayoutUnit::zero())
                + constraint_space.margin_strut.collapse();

            // Element's border-box top would naturally be at: base + margin_top
            // Clearance pushes it to at least clearance_floor
            // The final position is the max of these two
            let block_offset = if constraint_space.margins_already_applied {
                base_offset.max(clearance_floor)
            } else {
                (base_offset + sides.margin_top).max(clearance_floor)
            };

            return (BfcOffset::new(inline_offset, Some(block_offset)), false);
        }

        // Check if we can collapse margins with parent
        let can_collapse_top =
            sides.padding_top == LayoutUnit::zero() && sides.border_top == LayoutUnit::zero();

        // If parent established a new BFC and there are no sibling margins to collapse with,
        // margins don't collapse - add them as spacing
        if constraint_space.is_new_formatting_context && constraint_space.margin_strut.is_empty() {
            let block_offset = if constraint_space.margins_already_applied {
                constraint_space.bfc_offset.block_offset
            } else {
                constraint_space
                    .bfc_offset
                    .block_offset
                    .map(|offset| offset + sides.margin_top)
            };
            return (BfcOffset::new(inline_offset, block_offset), false);
        }

        if can_collapse_top {
            // We can collapse top margins (no border/padding at top)
            // Add our top margin to the strut and resolve to find our border-box position
            let mut strut = constraint_space.margin_strut;
            if !constraint_space.margins_already_applied {
                strut.append(sides.margin_top);
            }
            let collapsed = strut.collapse();
            let block_offset = constraint_space
                .bfc_offset
                .block_offset
                .map(|offset| offset + collapsed);
            return (BfcOffset::new(inline_offset, block_offset), false);
        }

        // Cannot collapse top margins with parent (have padding/border)
        // But we still need to collapse our top margin with any accumulated sibling margins
        let mut strut = constraint_space.margin_strut;
        if !constraint_space.margins_already_applied {
            strut.append(sides.margin_top);
        }
        let collapsed = strut.collapse();
        let block_offset = constraint_space
            .bfc_offset
            .block_offset
            .map(|offset| offset + collapsed);

        (BfcOffset::new(inline_offset, block_offset), false)
    }
    pub(super) fn compute_initial_margin_strut(
        _constraint_space: &ConstraintSpace,
        sides: &BoxSides,
        establishes_bfc: bool,
        can_collapse_with_children: bool,
    ) -> MarginStrut {
        if establishes_bfc {
            MarginStrut::default()
        } else if can_collapse_with_children {
            // When can_collapse_with_children is true, the parent's margin should be
            // in the strut so the first child can collapse with it.
            // The parent's bfc_offset was computed INCLUDING this margin, but we
            // subtract it in compute_child_base_bfc_offset and pass it here instead.
            let mut strut = MarginStrut::default();
            strut.append(sides.margin_top);
            strut
        } else {
            // Parent has padding/border preventing collapse, start with empty strut
            MarginStrut::default()
        }
    }

    /// Compute the base BFC offset for children.
    pub(super) fn compute_child_base_bfc_offset(
        bfc_offset: BfcOffset,
        sides: &BoxSides,
        establishes_bfc: bool,
        can_collapse_with_children: bool,
    ) -> BfcOffset {
        if establishes_bfc {
            BfcOffset::root()
        } else if can_collapse_with_children {
            // When parent can collapse with children, the parent's bfc_offset includes
            // the parent's collapsed margin. To allow children to re-collapse with the
            // parent's margin, we subtract it here and pass it via initial_margin_strut.
            BfcOffset::new(
                bfc_offset.inline_offset,
                bfc_offset
                    .block_offset
                    .map(|offset| offset - sides.margin_top),
            )
        } else {
            BfcOffset::new(
                bfc_offset.inline_offset + sides.padding_left + sides.border_left,
                bfc_offset
                    .block_offset
                    .map(|offset| offset + sides.padding_top + sides.border_top),
            )
        }
    }
    pub(super) fn compute_end_margin_strut(params: &EndMarginStrutParams<'_>) -> MarginStrut {
        let can_collapse_bottom = params.sides.padding_bottom == LayoutUnit::zero()
            && params.sides.border_bottom == LayoutUnit::zero();
        let can_collapse_top = params.sides.padding_top == LayoutUnit::zero()
            && params.sides.border_top == LayoutUnit::zero();

        // Check if this is a self-collapsing element (no content/height, no children)
        let is_self_collapsing = params.block_size.abs() < 0.01
            && !params.state.first_inflow_child_seen
            && can_collapse_top
            && can_collapse_bottom
            && params.can_collapse_with_children;

        if is_self_collapsing {
            // Self-collapsing element: margins collapse THROUGH the element
            // The next sibling should see all margins that collapsed through this element
            // BUG FIX: Include the INCOMING margin strut (which contains parent/sibling margins)
            // along with this element's own top and bottom margins
            let mut strut = params.incoming_space.margin_strut;
            strut.append(params.sides.margin_top);
            strut.append(params.sides.margin_bottom);
            return strut;
        }

        // If we have children and can collapse bottom margins
        if params.state.first_inflow_child_seen && can_collapse_bottom {
            // Collapse our bottom margin with last child's end margin strut
            let mut strut = params.state.last_child_end_margin_strut;
            strut.append(params.sides.margin_bottom);
            return strut;
        }

        // If we cannot collapse bottom (have padding/border), only our own bottom margin
        if !can_collapse_bottom {
            let mut strut = MarginStrut::default();
            strut.append(params.sides.margin_bottom);
            return strut;
        }

        // No children and can collapse bottom (but not self-collapsing due to explicit height):
        // Return just the bottom margin
        let mut strut = MarginStrut::default();
        strut.append(params.sides.margin_bottom);
        strut
    }
    pub(super) fn resolve_parent_offset_if_needed(
        resolved_bfc_offset: &mut BfcOffset,
        child_result: &LayoutResult,
        first_inflow_child_seen: bool,
        can_collapse_with_children: bool,
    ) {
        if !first_inflow_child_seen && can_collapse_with_children {
            // Parent collapses with first child - they share the same BFC offset
            // The child's offset already includes all collapsed margins
            resolved_bfc_offset.block_offset = child_result.bfc_offset.block_offset;
        }
    }
}
