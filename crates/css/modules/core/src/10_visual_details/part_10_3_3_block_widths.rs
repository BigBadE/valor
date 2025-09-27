//! Spec: CSS 2.2 §10.3.3 Non-replaced block elements in normal flow — widths and margins
//! Horizontal solving implementation: width constraints and margin resolution.

use crate::chapter8::part_8_3_1_collapsing_margins as cm83;
use crate::chapter9::part_9_4_3_relative_positioning::apply_relative_offsets;
use crate::chapter10::part_10_1_containing_block as cb10;
use crate::{ChildLayoutCtx, CollapsedPos, Layouter};
use css_box::BoxSides;
use css_orchestrator::style_model::Clear;
use css_orchestrator::style_model::{BoxSizing, ComputedStyle, Float};
use js::NodeKey;
use log::debug;

/// Tuple of optional width constraints (specified, min, max) in border-box space.
pub type WidthConstraints = (Option<i32>, Option<i32>, Option<i32>);

/// Inputs captured for horizontal solving logs (diagnostics only).
#[derive(Clone, Copy)]
struct HorizInputs {
    /// Container content width in pixels.
    container_w: i32,
    /// Box sides (margins, padding, borders) for the element.
    sides: BoxSides,
    /// Raw input margin-left in pixels.
    in_margin_left: i32,
    /// Raw input margin-right in pixels.
    in_margin_right: i32,
    /// Whether margin-left is auto.
    left_auto: bool,
    /// Whether margin-right is auto.
    right_auto: bool,
}

#[inline]
fn compute_collapsed_top_for_child(
    ctx: &ChildLayoutCtx,
    margin_top_eff: i32,
    style: &ComputedStyle,
) -> i32 {
    if ctx.is_first_placed
        && ctx.parent_edge_collapsible
        && !ctx.ancestor_applied_at_edge_for_children
    {
        return 0i32;
    }
    cm83::compute_collapsed_vertical_margin_public(ctx, margin_top_eff, style)
}

/// Composite: compute collapsed top and initial position info for a child.
/// Bridges §8.3.1 (collapsed top), §10.1 (parent origin), §10.3.3 (horizontal), and §9.4.3 (relative offsets).
#[inline]
pub fn compute_collapsed_and_position_public(
    layouter: &Layouter,
    child_key: NodeKey,
    ctx: &ChildLayoutCtx,
    style: &ComputedStyle,
    sides: &BoxSides,
) -> CollapsedPos {
    let margin_top_eff = cm83::effective_child_top_margin_public(layouter, child_key, sides);
    let collapsed_top = compute_collapsed_top_for_child(ctx, margin_top_eff, style);
    // first-child diagnostics logged elsewhere
    let (parent_x, parent_y) = cb10::parent_content_origin(&ctx.metrics);
    let parent_right = parent_x.saturating_add(ctx.metrics.container_width);
    let (used_bb_w, child_x, _resolved_ml) = compute_horizontal_position_public(
        style,
        sides,
        parent_x,
        parent_right,
        (ctx.float_band_left, ctx.float_band_right),
    );
    let (x_adjust, y_adjust) = apply_relative_offsets(style);
    // Compute pre-clear collapsed position: always include y_cursor and collapsed_top.
    // The parent content origin already accounts for padding/border; y_cursor captures
    // leading-top adjustments and prior flow.
    let collapsed_pre_y = cm83::compute_y_position_public(parent_y, ctx.y_cursor, collapsed_top);
    let mut child_y = collapsed_pre_y;
    if ctx.is_first_placed {
        log::debug!(
            "[VERT-FIRST pre] key={child_key:?} parent_edge_collapsible={} parent_y={} y_cursor={} mt_eff={} collapsed_top={} clear={:?} floor_y={}",
            ctx.parent_edge_collapsible,
            parent_y,
            ctx.y_cursor,
            margin_top_eff,
            collapsed_top,
            style.clear,
            ctx.clearance_floor_y
        );
    }
    log::debug!(
        "[VERT-POS pre] key={child_key:?} parent_y={} y_cursor={} collapsed_top={} clear={:?} floor_y={} -> pre_y={}",
        parent_y,
        ctx.y_cursor,
        collapsed_top,
        style.clear,
        ctx.clearance_floor_y,
        child_y
    );
    let mut clear_lifted = false;
    if matches!(style.float, Float::None)
        && matches!(style.clear, Clear::Left | Clear::Right | Clear::Both)
        && ctx.clearance_floor_y > child_y
    {
        // If the parent's top edge is non-collapsible (padding/border present or the parent
        // establishes a BFC), the first in-flow child's clearance must not be influenced by
        // external floats. The placement loop masks floors at BFC boundaries, but as a guard,
        // do not lift the first in-flow child under a non-collapsible parent edge here.
        if !ctx.is_first_placed || ctx.parent_edge_collapsible {
            child_y = ctx.clearance_floor_y;
            clear_lifted = child_y > collapsed_pre_y;
        }
    }
    log::debug!(
        "[VERT-POS out] key={child_key:?} child_y={} used_bb_w={} child_x={} mt_eff={} bands=({}, {})",
        child_y,
        used_bb_w,
        child_x,
        margin_top_eff,
        ctx.float_band_left,
        ctx.float_band_right
    );
    CollapsedPos {
        margin_top_eff,
        collapsed_top,
        used_bb_w,
        child_x,
        child_y,
        x_adjust,
        y_adjust,
        clear_lifted,
    }
}

/// Sum horizontal paddings and borders in pixels (clamped to >= 0 per side).
#[inline]
fn sum_horizontal(style: &ComputedStyle) -> i32 {
    let pad = style.padding.left.max(0.0f32) + style.padding.right.max(0.0f32);
    let border = style.border_width.left.max(0.0f32) + style.border_width.right.max(0.0f32);
    (pad + border) as i32
}

/// Box-sizing aware used border-box width computation.
/// Spec: CSS 2.2 §10.3.3 + Box Sizing L3 (non-normative for conversion logic)
#[inline]
pub fn used_border_box_width(style: &ComputedStyle, fill_available_border_box_width: i32) -> i32 {
    let extras = sum_horizontal(style);
    let specified_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .width
            .map(|width_val| (width_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.width.map(|width_val| width_val as i32),
    };
    let min_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .min_width
            .map(|width_val| (width_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.min_width.map(|width_val| width_val as i32),
    };
    let max_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .max_width
            .map(|width_val| (width_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.max_width.map(|width_val| width_val as i32),
    };
    let mut out = specified_bb_opt.unwrap_or(fill_available_border_box_width);
    if let Some(min_bb) = min_bb_opt {
        out = out.max(min_bb);
    }
    if let Some(max_bb) = max_bb_opt {
        out = out.min(max_bb);
    }
    out.max(0i32)
}

/// Resolution context for the specified-width horizontal solving path.
#[derive(Clone, Copy)]
struct ConstrainedHorizCtx {
    /// Constrained border-box width (px).
    constrained_bb: i32,
    /// Whether margin-left is auto.
    left_auto: bool,
    /// Whether margin-right is auto.
    right_auto: bool,
    /// Container content width (px).
    container_content_width: i32,
    /// Resolved margin-left (px).
    margin_left_resolved: i32,
    /// Resolved margin-right (px).
    margin_right_resolved: i32,
}

/// Context for the width:auto horizontal solving path (CSS 2.2 §10.3.3).
#[derive(Clone, Copy)]
struct AutoHorizCtx {
    /// Whether margin-left is auto.
    left_auto: bool,
    /// Whether margin-right is auto.
    right_auto: bool,
    /// Container content width (px).
    container_content_width: i32,
    /// Resolved margin-left (px).
    margin_left_resolved: i32,
    /// Resolved margin-right (px).
    margin_right_resolved: i32,
    /// Optional minimum border-box width (px).
    min_bb_opt: Option<i32>,
    /// Optional maximum border-box width (px).
    max_bb_opt: Option<i32>,
}

/// Compute width constraints converted to border-box space based on the element's box-sizing.
/// Spec: CSS 2.2 §10.3.3 (constraints) — content-box vs border-box adjustments.
#[inline]
pub fn compute_width_constraints(style: &ComputedStyle, sides: &BoxSides) -> WidthConstraints {
    let extras = sides
        .padding_left
        .saturating_add(sides.padding_right)
        .saturating_add(sides.border_left)
        .saturating_add(sides.border_right);
    let specified_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .width
            .map(|width_val| (width_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.width.map(|width_val| width_val as i32),
    };
    let min_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .min_width
            .map(|min_val| (min_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.min_width.map(|min_val| min_val as i32),
    };
    let max_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .max_width
            .map(|max_val| (max_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.max_width.map(|max_val| max_val as i32),
    };
    (specified_bb_opt, min_bb_opt, max_bb_opt)
}

/// Clamp a width value in border-box space using optional min/max constraints.
/// Spec: CSS 2.2 §10.4 (min/max) — applied to the used width result.
#[inline]
pub fn clamp_width_to_min_max(
    width_value: i32,
    min_bb_opt: Option<i32>,
    max_bb_opt: Option<i32>,
) -> i32 {
    let mut out = width_value;
    if let Some(min_b) = min_bb_opt {
        out = out.max(min_b);
    }
    if let Some(max_b) = max_bb_opt {
        out = out.min(max_b);
    }
    out
}

/// Spec: §10.3.3 — Compute horizontal placement with float bands applied.
/// Returns `(used_border_box_width, child_x, resolved_margin_left)`.
#[inline]
pub fn compute_horizontal_position_public(
    style: &ComputedStyle,
    sides: &BoxSides,
    parent_x: i32,
    parent_content_right: i32,
    float_bands: (i32, i32),
) -> (i32, i32, i32) {
    let (float_band_left, float_band_right) = float_bands;
    let parent_content_width = parent_content_right.saturating_sub(parent_x).max(0i32);
    let available_width = parent_content_width
        .saturating_sub(float_band_left)
        .saturating_sub(float_band_right)
        .max(0i32);
    let (used_bb_w_raw, resolved_ml, _resolved_mr) = solve_block_horizontal(
        style,
        sides,
        available_width,
        sides.margin_left,
        sides.margin_right,
    );
    // For floats with specified width, do not collapse to 0 when bands consume all inline space.
    // Use the parent content width as the constraint for the specified-width path (CSS 2.2 §10.3.3 used width),
    // while still positioning against bands.
    let used_bb_w = if !matches!(style.float, Float::None) && style.width.is_some() {
        used_border_box_width(style, parent_content_width)
    } else {
        used_bb_w_raw
    };
    let child_x = match style.float {
        Float::Left => parent_x.saturating_add(float_band_left),
        Float::Right => parent_content_right
            .saturating_sub(float_band_right)
            .saturating_sub(used_bb_w),
        Float::None => parent_x
            .saturating_add(float_band_left)
            .saturating_add(resolved_ml),
    };
    debug!(
        "[HORIZ-POS] float={:?} parent=[x={}, right={}, w={}] bands(L={},R={}) avail_w={} used_bb_w={} child_x={} ml_res={}",
        style.float,
        parent_x,
        parent_content_right,
        parent_content_width,
        float_band_left,
        float_band_right,
        available_width,
        used_bb_w,
        child_x,
        resolved_ml
    );
    (used_bb_w, child_x, resolved_ml)
}

/// Solve used border-box width and horizontal margins together for a non-replaced block in normal flow.
/// Implements CSS 2.2 §10.3.3 for horizontal dimensions.
#[inline]
pub fn solve_block_horizontal(
    style: &ComputedStyle,
    sides: &BoxSides,
    container_content_width: i32,
    margin_left_in: i32,
    margin_right_in: i32,
) -> (i32, i32, i32) {
    let (specified_bb_opt, min_bb_opt, max_bb_opt) = compute_width_constraints(style, sides);
    let left_auto = false;
    let right_auto = false;
    let margin_left_resolved = margin_left_in;
    let margin_right_resolved = margin_right_in;

    if specified_bb_opt.is_some() {
        let constrained = used_border_box_width(style, container_content_width);
        let ctx = ConstrainedHorizCtx {
            constrained_bb: constrained,
            left_auto,
            right_auto,
            container_content_width,
            margin_left_resolved,
            margin_right_resolved,
        };
        let out = resolve_with_constrained_width(ctx);
        let inputs = HorizInputs {
            container_w: container_content_width,
            sides: *sides,
            in_margin_left: margin_left_in,
            in_margin_right: margin_right_in,
            left_auto,
            right_auto,
        };
        log_horiz("specified", &inputs, constrained, out);
        return out;
    }

    let ctx = AutoHorizCtx {
        left_auto,
        right_auto,
        container_content_width,
        margin_left_resolved,
        margin_right_resolved,
        min_bb_opt,
        max_bb_opt,
    };
    let out = resolve_auto_width(ctx);
    let inputs = HorizInputs {
        container_w: container_content_width,
        sides: *sides,
        in_margin_left: margin_left_in,
        in_margin_right: margin_right_in,
        left_auto,
        right_auto,
    };
    log_horiz("auto", &inputs, out.0, out);
    out
}

/// Resolve margins given a constrained border-box width (specified width path).
/// Spec: CSS 2.2 §10.3.3 bullets for cases with specified width and auto margins.
fn resolve_with_constrained_width(mut ctx: ConstrainedHorizCtx) -> (i32, i32, i32) {
    let constrained = ctx.constrained_bb.max(0i32);
    if ctx.left_auto && ctx.right_auto {
        let remaining = diff_i32(ctx.container_content_width, constrained);
        let abs_remaining = if remaining >= 0i32 {
            remaining
        } else {
            0i32.saturating_sub(remaining)
        };
        let half = abs_remaining >> 1i32;
        if remaining >= 0i32 {
            ctx.margin_left_resolved = half;
            ctx.margin_right_resolved = abs_remaining.saturating_sub(half);
        } else {
            ctx.margin_left_resolved = 0i32.saturating_sub(half);
            ctx.margin_right_resolved = 0i32.saturating_sub(abs_remaining.saturating_sub(half));
        }
        return (
            constrained,
            ctx.margin_left_resolved,
            ctx.margin_right_resolved,
        );
    }
    if ctx.left_auto ^ ctx.right_auto {
        if ctx.left_auto {
            ctx.margin_left_resolved = diff_i32(
                diff_i32(ctx.container_content_width, constrained),
                ctx.margin_right_resolved,
            );
        } else {
            ctx.margin_right_resolved = diff_i32(
                diff_i32(ctx.container_content_width, constrained),
                ctx.margin_left_resolved,
            );
        }
        return (
            constrained,
            ctx.margin_left_resolved,
            ctx.margin_right_resolved,
        );
    }
    ctx.margin_right_resolved = diff_i32(
        diff_i32(ctx.container_content_width, constrained),
        ctx.margin_left_resolved,
    );
    (
        constrained,
        ctx.margin_left_resolved,
        ctx.margin_right_resolved,
    )
}

/// Resolve margins and compute border-box width for the width:auto path.
/// Spec: CSS 2.2 §10.3.3 final bullet for width:auto with different auto margin combinations.
fn resolve_auto_width(mut ctx: AutoHorizCtx) -> (i32, i32, i32) {
    let mut border_box_auto = if ctx.left_auto && ctx.right_auto {
        ctx.margin_left_resolved = 0i32;
        ctx.margin_right_resolved = 0i32;
        ctx.container_content_width
    } else if ctx.left_auto ^ ctx.right_auto {
        if ctx.left_auto {
            ctx.margin_left_resolved = 0i32;
            diff_i32(ctx.container_content_width, ctx.margin_right_resolved)
        } else {
            ctx.margin_right_resolved = 0i32;
            diff_i32(ctx.container_content_width, ctx.margin_left_resolved)
        }
    } else {
        let tmp = diff_i32(ctx.container_content_width, ctx.margin_left_resolved);
        diff_i32(tmp, ctx.margin_right_resolved)
    };
    border_box_auto = clamp_width_to_min_max(border_box_auto, ctx.min_bb_opt, ctx.max_bb_opt);
    (
        border_box_auto.max(0i32),
        ctx.margin_left_resolved,
        ctx.margin_right_resolved,
    )
}

/// Compute `lhs - rhs` allowing negative results using saturating ops.
#[inline]
const fn diff_i32(lhs: i32, rhs: i32) -> i32 {
    if lhs >= rhs {
        lhs.saturating_sub(rhs)
    } else {
        0i32.saturating_sub(rhs.saturating_sub(lhs))
    }
}

/// Log a single horizontal solve step for diagnostics.
#[inline]
fn log_horiz(path: &str, inputs: &HorizInputs, width_in: i32, out: (i32, i32, i32)) {
    debug!(
        "[HORIZ {path}] cont_w={} extras(pl,pr,bl,br)=({},{},{},{}) in(ml={},mr={}) auto(l={},r={}) width_in={} -> out(width={}, ml={}, mr={})",
        inputs.container_w,
        inputs.sides.padding_left,
        inputs.sides.padding_right,
        inputs.sides.border_left,
        inputs.sides.border_right,
        inputs.in_margin_left,
        inputs.in_margin_right,
        inputs.left_auto,
        inputs.right_auto,
        width_in,
        out.0,
        out.1,
        out.2,
    );
}
