//! Horizontal solving per CSS 2.2 §10.3.3 (Non-replaced block elements in normal flow)
//!
//! Each function below includes a spec pointer and a short note of the exact
//! clause implemented. Line anchors reference the HTML/CSS 2.2 spec rendering
//! available at <https://www.w3.org/TR/CSS22/visudet.html#blockwidth>
//!
//! Order of items follows the spec evaluation order: constraints -> specified
//! width path -> auto width path -> logging.

use crate::sizing::used_border_box_width;
use css_box::BoxSides;
use log::debug;
use style_engine::{BoxSizing, ComputedStyle};

/// Tuple of optional width constraints (specified, min, max) in border-box space.
pub type WidthConstraints = (Option<i32>, Option<i32>, Option<i32>);

/// Inputs captured for horizontal solving logs (diagnostics only).
#[derive(Clone, Copy)]
struct HorizInputs {
    /// Container content width.
    container_w: i32,
    /// Box sides used (padding/border) for diagnostics.
    sides: BoxSides,
    /// Original author-specified margin-left value (may be negative).
    in_margin_left: i32,
    /// Original author-specified margin-right value (may be negative).
    in_margin_right: i32,
    /// Whether margin-left was 'auto'.
    left_auto: bool,
    /// Whether margin-right was 'auto'.
    right_auto: bool,
}

/// Resolution context for the specified-width horizontal solving path.
#[derive(Clone, Copy)]
struct ConstrainedHorizCtx {
    /// Final constrained border-box width.
    constrained_bb: i32,
    /// Whether margin-left is auto.
    left_auto: bool,
    /// Whether margin-right is auto.
    right_auto: bool,
    /// Container content width.
    container_content_width: i32,
    /// Current margin-left value.
    margin_left_resolved: i32,
    /// Current margin-right value.
    margin_right_resolved: i32,
}

/// Context for the width:auto horizontal solving path (CSS 2.2 §10.3.3).
#[derive(Clone, Copy)]
struct AutoHorizCtx {
    /// Whether margin-left is auto.
    left_auto: bool,
    /// Whether margin-right is auto.
    right_auto: bool,
    /// Container content width.
    container_content_width: i32,
    /// Current margin-left value.
    margin_left_resolved: i32,
    /// Current margin-right value.
    margin_right_resolved: i32,
    /// Minimum border-box width constraint (if any).
    min_bb_opt: Option<i32>,
    /// Maximum border-box width constraint (if any).
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
    let left_auto = style.margin_left_auto;
    let right_auto = style.margin_right_auto;
    let margin_left_resolved = margin_left_in;
    let margin_right_resolved = margin_right_in;

    if specified_bb_opt.is_some() {
        // Spec path: specified width present -> compute used width and resolve margins accordingly.
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

    // Spec path: width:auto
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
    // Over-constrained: adjust margin-right (assuming LTR; no direction support in shim yet).
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
        let delta = rhs.saturating_sub(lhs);
        0i32.saturating_sub(delta)
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
