// Per-child placement composite
// Wraps §10.3.3 (horizontal/position), §10.6.3 (heights/margins), rect insertion and logging.

use crate::INITIAL_CONTAINING_BLOCK_HEIGHT;
use crate::chapter10::part_10_1_containing_block as cb10;
use crate::chapter10::part_10_6_3_height_of_blocks;
use crate::{
    ChildLayoutCtx, CollapsedPos, HeightsAndMargins, HeightsCtx, LayoutRect, Layouter, VertCommit,
};
use css_box::{BoxSides, compute_box_sides};
use css_orchestrator::style_model::{ComputedStyle, Position};
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

/// Containing block geometry used for positioned layout.
#[derive(Clone, Copy)]
struct ContainingBlock {
    /// Left/top origin in parent or viewport space.
    x: i32,
    /// Left/top origin in parent or viewport space.
    y: i32,
    /// Available width for positioned offset resolution.
    width: i32,
    /// Available height for positioned offset resolution.
    height: i32,
}

/// Compute the containing block for an absolute/fixed positioned element.
///
/// Absolute: nearest positioned ancestor's padding-box; fallback to parent content-box.
/// Fixed: viewport approximation.
fn containing_block(
    layouter: &Layouter,
    ctx: &ChildLayoutCtx,
    child_key: NodeKey,
    style: &ComputedStyle,
) -> ContainingBlock {
    if matches!(style.position, Position::Fixed) {
        return ContainingBlock {
            x: 0,
            y: 0,
            width: ctx.metrics.total_border_box_width,
            height: INITIAL_CONTAINING_BLOCK_HEIGHT,
        };
    }
    // Walk up to find nearest positioned ancestor
    let mut current: Option<NodeKey> = Some(child_key);
    let mut nearest_positioned: Option<NodeKey> = None;
    // Helper to find parent by scanning children map (small trees in tests)
    let find_parent = |lay: &Layouter, needle: NodeKey| -> Option<NodeKey> {
        // Collect keys to avoid iterating over hash-based type directly
        let children_vec: Vec<_> = lay.children.iter().collect();
        for (parent_key, children) in children_vec {
            if children.contains(&needle) {
                return Some(*parent_key);
            }
        }
        None
    };
    while let Some(node) = current {
        if let Some(parent) = find_parent(layouter, node) {
            if let Some(parent_style) = layouter.computed_styles.get(&parent)
                && !matches!(parent_style.position, Position::Static)
            {
                nearest_positioned = Some(parent);
                break;
            }
            current = Some(parent);
        } else {
            break;
        }
    }
    if let Some(ancestor) = nearest_positioned {
        // Use ancestor padding box as CB if we have its rect; otherwise fall back to parent content origin
        if let Some(rect) = layouter.rects.get(&ancestor)
            && let Some(ancestor_style) = layouter.computed_styles.get(&ancestor)
        {
            let sides = compute_box_sides(ancestor_style);
            // Offsets for absolute positioning are resolved from the padding edge of the containing block.
            // The origin is therefore the border edge plus border thickness (do NOT add padding here).
            let x = (rect.x as i32).saturating_add(sides.border_left);
            let y = (rect.y as i32).saturating_add(sides.border_top);
            // Percentages are resolved against the padding box size: subtract borders only.
            let width = (rect.width as i32)
                .saturating_sub(sides.border_left)
                .saturating_sub(sides.border_right)
                .max(0i32);
            let height = (rect.height as i32)
                .saturating_sub(sides.border_top)
                .saturating_sub(sides.border_bottom)
                .max(0i32);
            return ContainingBlock {
                x,
                y,
                width,
                height,
            };
        }
    }
    // Fallback: parent content-box from context
    let (parent_x, parent_y) = cb10::parent_content_origin(&ctx.metrics);
    ContainingBlock {
        x: parent_x,
        y: parent_y,
        width: ctx.metrics.container_width,
        height: 0,
    }
}

/// Resolve a single offset by preferring percentage over pixels and clamping to >= 0.
fn resolve_offset(px_opt: Option<f32>, pct_opt: Option<f32>, total: i32) -> i32 {
    pct_opt.map_or_else(
        || px_opt.map_or(0i32, |value| value.round() as i32).max(0i32),
        |fraction| ((fraction * total as f32).round() as i32).max(0i32),
    )
}

/// Resolved offsets in pixels from the containing block edges.
#[derive(Clone, Copy)]
struct ResolvedOffsets {
    /// Top offset from containing block's top edge, in pixels (>= 0).
    top: i32,
    /// Left offset from containing block's left edge, in pixels (>= 0).
    left: i32,
    /// Right offset from containing block's right edge, in pixels (>= 0).
    right: i32,
    /// Bottom offset from containing block's bottom edge, in pixels (>= 0).
    bottom: i32,
}

/// Resolve all four offsets from `ComputedStyle` against the containing block.
fn resolve_all_offsets(style: &ComputedStyle, cblock: &ContainingBlock) -> ResolvedOffsets {
    ResolvedOffsets {
        top: resolve_offset(style.top, style.top_percent, cblock.height),
        left: resolve_offset(style.left, style.left_percent, cblock.width),
        right: resolve_offset(style.right, style.right_percent, cblock.width),
        bottom: resolve_offset(style.bottom, style.bottom_percent, cblock.height),
    }
}

/// Compact bitset indicating which offsets were specified by the author.
#[derive(Clone, Copy)]
struct OffsetMask(u8);

impl OffsetMask {
    /// Bit for left offset presence.
    const LEFT: u8 = 1 << 0;
    /// Bit for right offset presence.
    const RIGHT: u8 = 1 << 1;
    /// Bit for top offset presence.
    const TOP: u8 = 1 << 2;
    /// Bit for bottom offset presence.
    const BOTTOM: u8 = 1 << 3;

    #[inline]
    /// Create a new mask from raw bits.
    const fn new(bits: u8) -> Self {
        Self(bits)
    }

    #[inline]
    /// True if left offset is specified.
    const fn has_left(self) -> bool {
        (self.0 & Self::LEFT) != 0
    }
    #[inline]
    /// True if right offset is specified.
    const fn has_right(self) -> bool {
        (self.0 & Self::RIGHT) != 0
    }
    #[inline]
    /// True if top offset is specified.
    const fn has_top(self) -> bool {
        (self.0 & Self::TOP) != 0
    }
    #[inline]
    /// True if bottom offset is specified.
    const fn has_bottom(self) -> bool {
        (self.0 & Self::BOTTOM) != 0
    }
}

/// Compute used width/height for positioned layout honoring opposite-offset rules.
fn compute_used_sizes(
    style: &ComputedStyle,
    cblock: &ContainingBlock,
    mask: OffsetMask,
    offsets: ResolvedOffsets,
) -> (i32, i32) {
    let mut used_w = style
        .width
        .map_or(0i32, |width_px| width_px.round() as i32)
        .max(0i32);
    let mut used_h = style
        .height
        .map_or(0i32, |height_px| height_px.round() as i32)
        .max(0i32);
    if style.width.is_none() && mask.has_left() && mask.has_right() {
        used_w = cblock
            .width
            .saturating_sub(offsets.left)
            .saturating_sub(offsets.right)
            .max(0i32);
    } else if style.width.is_none() && (mask.has_left() ^ mask.has_right()) {
        // Shrink-to-fit fallback: occupy remaining width from the specified edge
        let remaining = if mask.has_left() {
            cblock.width.saturating_sub(offsets.left)
        } else {
            cblock.width.saturating_sub(offsets.right)
        };
        used_w = remaining.max(0i32);
    }
    if style.height.is_none() && mask.has_top() && mask.has_bottom() {
        used_h = cblock
            .height
            .saturating_sub(offsets.top)
            .saturating_sub(offsets.bottom)
            .max(0i32);
    } else if style.height.is_none() && (mask.has_top() ^ mask.has_bottom()) {
        let remaining = if mask.has_top() {
            cblock.height.saturating_sub(offsets.top)
        } else {
            cblock.height.saturating_sub(offsets.bottom)
        };
        used_h = remaining.max(0i32);
    }
    (used_w, used_h)
}

/// Choose the final (x, y) from the available edges and used size.
const fn choose_position(
    cblock: &ContainingBlock,
    used_width: i32,
    used_height: i32,
    offsets: ResolvedOffsets,
    mask: OffsetMask,
) -> (i32, i32) {
    let x_from_left = cblock.x.saturating_add(offsets.left);
    let y_from_top = cblock.y.saturating_add(offsets.top);
    let x_from_right = cblock
        .x
        .saturating_add(cblock.width)
        .saturating_sub(offsets.right)
        .saturating_sub(used_width);
    let y_from_bottom = cblock
        .y
        .saturating_add(cblock.height)
        .saturating_sub(offsets.bottom)
        .saturating_sub(used_height);
    let child_x = if mask.has_left() {
        x_from_left
    } else if mask.has_right() {
        x_from_right
    } else {
        x_from_left
    };
    let child_y = if mask.has_top() {
        y_from_top
    } else if mask.has_bottom() {
        y_from_bottom
    } else {
        y_from_top
    };
    (child_x, child_y)
}

/// Positioned rectangle payload for commit.
#[derive(Clone, Copy)]
struct PositionedBox {
    /// X coordinate of the positioned box (border-box), in px.
    x: i32,
    /// Y coordinate of the positioned box (border-box), in px.
    y: i32,
    /// Used border-box width of the positioned box, in px.
    width: i32,
    /// Used border-box height of the positioned box, in px.
    height: i32,
}

/// Commit a positioned rectangle and return a zero-flow contribution `PlacedBlock`.
fn commit_positioned(
    layouter: &mut Layouter,
    ctx: &ChildLayoutCtx,
    child_key: NodeKey,
    sides: BoxSides,
    rect: PositionedBox,
) -> PlacedBlock {
    emit_vert_commit(
        layouter,
        ctx,
        CommitInputs {
            child_key,
            sides,
            margin_top_eff: 0i32,
            eff_bottom: 0i32,
            is_empty: false,
            collapsed_top: 0i32,
            child_x: rect.x,
            child_y: rect.y,
            used_bb_w: rect.width,
            x_adjust: 0i32,
            y_adjust: 0i32,
            computed_h: rect.height,
        },
    );
    PlacedBlock {
        y: ctx.y_cursor,
        content_height: 0i32,
        outgoing_bottom_margin: 0i32,
        collapsed_top: 0i32,
        clear_lifted: false,
        parent_edge_collapsible: ctx.parent_edge_collapsible,
        leading_collapse_contrib: 0i32,
    }
}

/// Handle out-of-flow positioned boxes (absolute/fixed). Returns `Some(PlacedBlock)` if handled.
fn handle_out_of_flow_positioned(
    layouter: &mut Layouter,
    child_key: NodeKey,
    ctx: &ChildLayoutCtx,
    style: &ComputedStyle,
    sides: BoxSides,
) -> Option<PlacedBlock> {
    if !matches!(style.position, Position::Absolute | Position::Fixed) {
        return None;
    }
    let cblock = containing_block(layouter, ctx, child_key, style);
    let offsets = resolve_all_offsets(style, &cblock);
    let mut bits: u8 = 0;
    if style.left.is_some() || style.left_percent.is_some() {
        bits |= OffsetMask::LEFT;
    }
    if style.right.is_some() || style.right_percent.is_some() {
        bits |= OffsetMask::RIGHT;
    }
    if style.top.is_some() || style.top_percent.is_some() {
        bits |= OffsetMask::TOP;
    }
    if style.bottom.is_some() || style.bottom_percent.is_some() {
        bits |= OffsetMask::BOTTOM;
    }
    let mask = OffsetMask::new(bits);
    let (used_width, used_height) = compute_used_sizes(style, &cblock, mask, offsets);
    let (child_x, child_y) = choose_position(&cblock, used_width, used_height, offsets, mask);
    let rect = PositionedBox {
        x: child_x,
        y: child_y,
        width: used_width,
        height: used_height,
    };
    Some(commit_positioned(layouter, ctx, child_key, sides, rect))
}

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

/// Compute the parent leading-collapse contribution for the first in-flow child.
const fn compute_leading_collapse_contrib(_ctx: &ChildLayoutCtx, _clear_lifted: bool) -> i32 {
    // Parent origin and child metrics already incorporate any applied leading-top shift.
    // Reporting a non-zero contribution would double-subtract from the parent content height.
    0i32
}

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
    if let Some(result) = handle_out_of_flow_positioned(layouter, child_key, &ctx, &style, sides) {
        return result;
    }
    let CollapsedPos {
        margin_top_eff,
        collapsed_top,
        mut used_bb_w,
        child_x,
        child_y,
        x_adjust,
        y_adjust,
        clear_lifted,
    } = layouter.compute_collapsed_and_position(child_key, &ctx, &style, &sides);

    // Override width for replaced elements (form controls) per CSS 2.2 §10.3.2
    // "If 'width' has a computed value of 'auto', and the element has an intrinsic width,
    // then that intrinsic width is the used value of 'width'."
    if style.width.is_none() {
        if let Some(intrinsic_w) =
            part_10_6_3_height_of_blocks::intrinsic_width_for_form_control_public(
                layouter, child_key, &style, false, // Not a flex item in normal flow
            )
        {
            used_bb_w = intrinsic_w;
        }
    }

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
