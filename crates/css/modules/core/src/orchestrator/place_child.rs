// Per-child placement composite
// Wraps §10.3.3 (horizontal/position), §10.6.3 (heights/margins), rect insertion and logging.

use crate::chapter10::part_10_1_containing_block as cb10;
use crate::{
    ChildLayoutCtx, CollapsedPos, HeightsAndMargins, HeightsCtx, LayoutRect, Layouter, VertCommit,
};
use css_box::{BoxSides, compute_box_sides};
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;

/// Result of placing a single block-level child.
/// Source of truth for vertical composition. Do not re-derive from y deltas elsewhere.
#[derive(Clone, Copy)]
pub struct PlacedBlock {
    /// Final y position (margin edge) of the child.
    pub y: i32,
    /// Final content/border-box height of the child.
    pub content_height: i32,
    /// Outgoing bottom margin to the next sibling (signed; consumers apply policy).
    pub outgoing_bottom_margin: i32,
    /// Collapsed-top applied at this edge (signed).
    pub collapsed_top: i32,
    /// Whether clearance lifted the child above the collapsed-top pre-position.
    pub clear_lifted: bool,
    /// Whether the parent top edge is collapsible (for first in-flow child composition rules).
    pub parent_edge_collapsible: bool,
    /// Contribution to leading top collapse to subtract from parent content height.
    /// Non-zero only for the first in-flow child when the parent top edge is collapsible
    /// and clearance did not lift the child.
    pub leading_collapse_contrib: i32,
}

/// Inputs required to emit a vertical commit/log and child rect to the layouter.
#[derive(Clone, Copy)]
struct CommitInputs {
    /// Child node key.
    child_key: NodeKey,
    /// Child box sides used for logging (raw margins).
    sides: BoxSides,
    /// Effective margin-top used at this edge.
    margin_top_eff: i32,
    /// Effective bottom margin after internal propagation.
    eff_bottom: i32,
    /// Whether the child is effectively empty for collapsing.
    is_empty: bool,
    /// Collapsed-top offset applied at this edge.
    collapsed_top: i32,
    /// Child x position (margin edge).
    child_x: i32,
    /// Child y position (margin edge).
    child_y: i32,
    /// Used border-box width.
    used_bb_w: i32,
    /// Relative offset x from position: relative.
    x_adjust: i32,
    /// Relative offset y from position: relative.
    y_adjust: i32,
    /// Computed border-box height.
    computed_h: i32,
}

#[inline]
/// Emit diagnostics and insert the child's rectangle into the layouter.
fn emit_vert_commit(layouter: &mut Layouter, ctx: &ChildLayoutCtx, inputs: CommitInputs) {
    layouter.commit_vert(VertCommit {
        index: ctx.index,
        prev_mb: ctx.previous_bottom_margin,
        margin_top_raw: inputs.sides.margin_top,
        margin_top_eff: inputs.margin_top_eff,
        eff_bottom: inputs.eff_bottom,
        is_empty: inputs.is_empty,
        collapsed_top: inputs.collapsed_top,
        parent_origin_y: cb10::parent_content_origin(&ctx.metrics).1,
        y_position: inputs.child_y,
        y_cursor_in: ctx.y_cursor,
        leading_top_applied: if ctx.parent_edge_collapsible && ctx.is_first_placed {
            ctx.leading_top_applied
        } else {
            0
        },
        child_key: inputs.child_key,
        rect: LayoutRect {
            x: i32::saturating_add(inputs.child_x, inputs.x_adjust) as f32,
            y: i32::saturating_add(inputs.child_y, inputs.y_adjust) as f32,
            width: inputs.used_bb_w as f32,
            height: inputs.computed_h as f32,
        },
    });
}

#[inline]
/// Compute the parent leading-collapse contribution for the first in-flow child.
const fn compute_leading_collapse_contrib(_ctx: &ChildLayoutCtx, _clear_lifted: bool) -> i32 {
    // Parent origin and child metrics already incorporate any applied leading-top shift.
    // Reporting a non-zero contribution would double-subtract from the parent content height.
    0i32
}

#[inline]
/// Place a single block-level child and return a `PlacedBlock` bundle for vertical composition.
pub fn place_child_public(
    layouter: &mut Layouter,
    child_key: NodeKey,
    ctx: ChildLayoutCtx,
) -> PlacedBlock {
    let style = layouter
        .computed_styles
        .get(&child_key)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let sides = compute_box_sides(&style);
    let CollapsedPos {
        margin_top_eff,
        collapsed_top,
        used_bb_w,
        child_x,
        child_y,
        x_adjust,
        y_adjust,
        clear_lifted,
    } = layouter.compute_collapsed_and_position(child_key, &ctx, &style, &sides);

    let HeightsAndMargins {
        computed_h,
        eff_bottom,
        is_empty,
        margin_bottom_out,
    } = layouter.compute_heights_and_margins(HeightsCtx {
        child_key,
        style: &style,
        sides,
        child_x,
        child_y,
        used_bb_w,
        ctx: &ctx,
        margin_top_eff,
    });

    emit_vert_commit(
        layouter,
        &ctx,
        CommitInputs {
            child_key,
            sides,
            margin_top_eff,
            eff_bottom,
            is_empty,
            collapsed_top,
            child_x,
            child_y,
            used_bb_w,
            x_adjust,
            y_adjust,
            computed_h,
        },
    );

    // Compute leading collapse contribution via a dedicated helper.
    let leading_collapse_contrib = compute_leading_collapse_contrib(&ctx, clear_lifted);
    // Invariants: guard against misuse upstream.
    debug_assert!(
        ctx.parent_edge_collapsible || leading_collapse_contrib == 0i32,
        "leading_collapse_contrib must be 0 when parent edge is non-collapsible"
    );
    debug_assert!(
        !clear_lifted || leading_collapse_contrib == 0i32,
        "leading_collapse_contrib must be 0 when clearance lifted the child"
    );

    PlacedBlock {
        y: child_y,
        content_height: computed_h,
        outgoing_bottom_margin: margin_bottom_out,
        collapsed_top,
        clear_lifted,
        parent_edge_collapsible: ctx.parent_edge_collapsible,
        leading_collapse_contrib,
    }
}
