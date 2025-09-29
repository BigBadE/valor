//! Spec: CSS 2.2 §10.6.3 The height of blocks
//! Root height and used height computations.

use crate::LayoutRect;
use css_display::build_inline_context_with_filter;
use css_flexbox::{
    AlignContent as FlexAlignContent, AlignItems as FlexAlignItems, Axes as FlexAxes,
    CrossAndBaseline as FlexCrossAndBaseline, CrossContext as FlexCrossContext, CrossPlacement,
    FlexChild, FlexContainerInputs, FlexDirection as FlexDir, FlexItem, FlexPlacement,
    ItemRef as FlexItemRef, ItemStyle as FlexItemStyle, JustifyContent as FlexJustify,
    WritingMode as FlexWritingMode, collect_flex_items, layout_multi_line_with_cross,
    layout_single_line_with_cross, resolve_axes as flex_resolve_axes,
};
use css_orchestrator::style_model::{
    AlignContent as CoreAlignContent, AlignItems as CoreAlignItems, ComputedStyle,
    Display as CoreDisplay, FlexDirection as CoreFlexDirection, FlexWrap as CoreFlexWrap,
    JustifyContent as CoreJustify, Overflow as CoreOverflow, Position as CorePosition,
    WritingMode as CoreWritingMode,
};
use css_text::{collapse_whitespace, default_line_height_px};
use js::NodeKey;
use std::collections::{HashMap, HashSet};

use crate::LayoutNodeKind;

use crate::chapter8::part_8_3_1_collapsing_margins as cm83;
use crate::chapter9::part_9_4_1_block_formatting_context::establishes_block_formatting_context;
use crate::{
    ChildContentCtx, HeightExtras, HeightsAndMargins, HeightsCtx, HorizontalEdges, Layouter,
    RootHeightsCtx, TopEdges,
};

#[inline]
/// Compute inline baselines using inline-context grouping and default line-height.
/// Returns `(first_baseline, last_baseline)` in CSS px when inline content exists.
fn try_inline_baselines(layouter: &Layouter, node: NodeKey) -> Option<(f32, f32)> {
    // Gather flat children under this node
    let children = layouter.children.get(&node).cloned().unwrap_or_default();
    if children.is_empty() {
        return None;
    }
    // Build a quick node-kind map for whitespace skipping
    let mut kind_map: HashMap<NodeKey, LayoutNodeKind> = HashMap::new();
    for (key, kind, _kids) in layouter.snapshot() {
        kind_map.insert(key, kind);
    }
    let styles = &layouter.computed_styles;
    let parent_style = styles.get(&node);
    let skip_predicate = |node_key: NodeKey| -> bool {
        if let Some(LayoutNodeKind::InlineText { text }) = kind_map.get(&node_key).cloned() {
            collapse_whitespace(&text).is_empty()
        } else {
            false
        }
    };
    let lines = build_inline_context_with_filter(&children, styles, parent_style, skip_predicate);
    if lines.is_empty() {
        return None;
    }
    // Estimate baselines from default line-height
    let style = styles
        .get(&node)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let lh_px = default_line_height_px(&style) as f32;
    let first = lh_px.max(0.0);
    let last = (lines.len() as f32 * lh_px).max(first);
    Some((first, last))
}

// Tests moved to dedicated directory to keep clippy pedantic clean for library code.

/// Triplet describing an item's cross size constraints `(size, min, max)`.
type CrossTriplet = (f32, f32, f32);
/// Container context for flex layout: `(origin_xy, direction, axes, container_main_size, main_gap)`.
type FlexContainerCtx = ((i32, i32), FlexDir, FlexAxes, f32, f32);
/// Baseline metrics vector per item: `(first_baseline, last_baseline)` or `None` when unavailable.
type BaselineVec = Vec<Option<(f32, f32)>>;
/// Triple of flex inputs returned by `build_flex_item_inputs`.
type FlexInputsTriple = (Vec<FlexChild>, Vec<CrossTriplet>, BaselineVec);

/// Compute content height and root border-box height.
#[inline]
pub fn compute_root_heights(layouter: &Layouter, ctx: RootHeightsCtx) -> (i32, i32) {
    let content_origin = ctx
        .root_y
        .saturating_add(ctx.metrics.border_top)
        .saturating_add(ctx.metrics.padding_top);
    let root_style = layouter
        .computed_styles
        .get(&ctx.root)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let padding_bottom = root_style.padding.bottom.max(0.0f32) as i32;
    let border_bottom = root_style.border_width.bottom.max(0.0f32) as i32;
    let bottom_edge_collapsible = padding_bottom == 0i32
        && border_bottom == 0i32
        && !establishes_block_formatting_context(&root_style);
    let content_height = ctx.content_bottom.map_or(0i32, |bottom_value| {
        bottom_value.saturating_sub(content_origin).max(0i32)
    });

    log::debug!(
        "[ROOT-HEIGHT] root={:?} origin_y={} content_bottom={:?} last_pos_mb={} bottom_edge_collapsible={} pb={} bb={} -> content_h={}",
        ctx.root,
        content_origin,
        ctx.content_bottom,
        ctx.root_last_pos_mb,
        bottom_edge_collapsible,
        padding_bottom,
        border_bottom,
        content_height
    );
    let root_height_border_box = content_height
        .saturating_add(ctx.metrics.padding_top)
        .saturating_add(padding_bottom)
        .saturating_add(ctx.metrics.border_top)
        .saturating_add(border_bottom)
        .max(0i32);
    (content_height, root_height_border_box)
}

#[inline]
/// Compute container origin, axes, and main inputs for flex layout.
fn container_layout_context(
    cctx: ChildContentCtx,
    container_style: &ComputedStyle,
) -> FlexContainerCtx {
    let metrics = Layouter::build_child_metrics(
        cctx.used_border_box_width,
        HorizontalEdges {
            padding_left: cctx.sides.padding_left,
            padding_right: cctx.sides.padding_right,
            border_left: cctx.sides.border_left,
            border_right: cctx.sides.border_right,
        },
        TopEdges {
            padding_top: cctx.sides.padding_top,
            border_top: cctx.sides.border_top,
        },
        cctx.x,
        cctx.y,
    );
    let origin = (
        cctx.x.saturating_add(
            cctx.sides
                .border_left
                .saturating_add(cctx.sides.padding_left),
        ),
        cctx.y
            .saturating_add(cctx.sides.border_top.saturating_add(cctx.sides.padding_top)),
    );
    let direction = match container_style.flex_direction {
        CoreFlexDirection::Column => FlexDir::Column,
        CoreFlexDirection::Row => FlexDir::Row,
    };
    let writing_mode = match container_style.writing_mode {
        CoreWritingMode::HorizontalTb => FlexWritingMode::HorizontalTb,
        CoreWritingMode::VerticalRl => FlexWritingMode::VerticalRl,
        CoreWritingMode::VerticalLr => FlexWritingMode::VerticalLr,
    };
    let axes = flex_resolve_axes(direction, writing_mode);
    let container_main_size: f32 = if axes.main_is_inline {
        // Main axis maps to inline axis: use container inline size (width in HorizontalTb)
        metrics.container_width as f32
    } else {
        // Main axis maps to block axis: use computed height when specified; otherwise treat as unbounded
        container_style.height.unwrap_or(1_000_000.0).max(0.0)
    };
    let main_gap = match container_style.flex_direction {
        CoreFlexDirection::Column => container_style.row_gap_percent.map_or_else(
            || container_style.row_gap.max(0.0),
            |percent| (percent * container_main_size).max(0.0),
        ),
        CoreFlexDirection::Row => container_style.column_gap_percent.map_or_else(
            || container_style.column_gap.max(0.0),
            |percent| (percent * container_main_size).max(0.0),
        ),
    };
    (origin, direction, axes, container_main_size, main_gap)
}

#[inline]
/// Collect normalized flex item shells from children of the container.
fn collect_item_shells(layouter: &Layouter, parent: NodeKey) -> Vec<(FlexItemRef, FlexItemStyle)> {
    // Only consider element block children for flex item collection; ignore text/anonymous nodes.
    let child_list = layouter.children.get(&parent).cloned().unwrap_or_default();
    let mut block_nodes: HashSet<NodeKey> = HashSet::with_capacity(child_list.len());
    for (key, kind, _kids) in layouter.snapshot() {
        if matches!(kind, LayoutNodeKind::Block { .. }) {
            block_nodes.insert(key);
        }
    }
    let mut out: Vec<(FlexItemRef, FlexItemStyle)> = Vec::with_capacity(child_list.len());
    for child in &child_list {
        if !block_nodes.contains(child) {
            continue;
        }
        let style = layouter
            .computed_styles
            .get(child)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let is_none = matches!(style.display, CoreDisplay::None);
        let out_of_flow = !matches!(
            style.position,
            CorePosition::Static | CorePosition::Relative
        );
        out.push((
            FlexItemRef(child.0),
            FlexItemStyle {
                is_none,
                out_of_flow,
            },
        ));
    }
    out
}

/// Composite: compute heights and outgoing margins for a child.
/// Mirrors the former lib.rs logic, exposed here to keep lib.rs thin.
#[inline]
pub fn compute_heights_and_margins_public(
    layouter: &mut Layouter,
    hctx: HeightsCtx<'_>,
) -> HeightsAndMargins {
    let (content_h_inner, _last_out_mb) = layouter.compute_child_content_height(ChildContentCtx {
        key: hctx.child_key,
        used_border_box_width: hctx.used_bb_w,
        sides: hctx.sides,
        x: hctx.child_x,
        y: hctx.child_y,
        ancestor_applied_at_edge: hctx.ctx.ancestor_applied_at_edge_for_children,
    });
    let content_h = content_h_inner;
    let computed_h = compute_used_height(
        layouter,
        hctx.style,
        hctx.child_key,
        HeightExtras {
            padding_top: hctx.sides.padding_top,
            padding_bottom: hctx.sides.padding_bottom,
            border_top: hctx.sides.border_top,
            border_bottom: hctx.sides.border_bottom,
        },
        content_h,
    );
    let eff_bottom =
        cm83::effective_child_bottom_margin_public(layouter, hctx.child_key, &hctx.sides);
    let is_empty =
        layouter.is_effectively_empty_box(hctx.style, &hctx.sides, computed_h, hctx.child_key);
    let margin_bottom_out = if is_empty && hctx.ctx.is_first_placed {
        cm83::compute_first_placed_empty_margin_bottom(
            hctx.ctx.previous_bottom_margin,
            hctx.ctx.parent_self_top_margin,
            hctx.margin_top_eff,
            eff_bottom,
        )
    } else {
        cm83::compute_margin_bottom_out(hctx.margin_top_eff, eff_bottom, is_empty)
    };
    HeightsAndMargins {
        computed_h,
        eff_bottom,
        is_empty,
        margin_bottom_out,
    }
}

/// Compute used height for a block child, applying box extras when height is auto and
/// falling back to a single line height if there is inline text and overall height is 0.
#[inline]
pub fn compute_used_height(
    layouter: &Layouter,
    style: &ComputedStyle,
    child_key: NodeKey,
    extras: HeightExtras,
    child_content_height: i32,
) -> i32 {
    // Inline: used_border_box_height (moved from sizing.rs). Spec: CSS 2.2 §10.6.3 + Box Sizing L3
    #[inline]
    fn sum_vertical(style: &ComputedStyle) -> i32 {
        let pad = style.padding.top.max(0.0f32) + style.padding.bottom.max(0.0f32);
        let border = style.border_width.top.max(0.0f32) + style.border_width.bottom.max(0.0f32);
        (pad + border) as i32
    }
    #[inline]
    fn used_border_box_height(style: &ComputedStyle) -> i32 {
        use css_orchestrator::style_model::BoxSizing;
        let extras = sum_vertical(style);
        let specified_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .height
                .map(|height_val| (height_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.height.map(|height_val| height_val as i32),
        };
        let min_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .min_height
                .map(|height_val| (height_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.min_height.map(|height_val| height_val as i32),
        };
        let max_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .max_height
                .map(|height_val| (height_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.max_height.map(|height_val| height_val as i32),
        };
        let mut out = specified_bb_opt.unwrap_or(0i32);
        if let Some(min_bb) = min_bb_opt {
            out = out.max(min_bb);
        }
        if let Some(max_bb) = max_bb_opt {
            out = out.min(max_bb);
        }
        out.max(0i32)
    }
    let mut computed_height = used_border_box_height(style);
    if style.height.is_none() {
        computed_height = child_content_height
            .saturating_add(extras.padding_top)
            .saturating_add(extras.padding_bottom)
            .saturating_add(extras.border_top)
            .saturating_add(extras.border_bottom);
        if computed_height == 0i32 && layouter.has_inline_text_descendant(child_key) {
            computed_height = default_line_height_px(style);
        }
    }
    computed_height
}

/// Build child container metrics and compute raw content height by laying out descendants.
/// Returns `(content_height, last_positive_bottom_margin)`.
#[inline]
pub fn compute_child_content_height(layouter: &mut Layouter, cctx: ChildContentCtx) -> (i32, i32) {
    let container_style = layouter
        .computed_styles
        .get(&cctx.key)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    if matches!(
        container_style.display,
        CoreDisplay::Flex | CoreDisplay::InlineFlex
    ) {
        return flex_child_content_height(layouter, cctx, &container_style);
    }
    block_child_content_height(layouter, cctx)
}

#[inline]
/// Block fallback: lay out the container's children in block formatting context.
fn block_child_content_height(layouter: &mut Layouter, cctx: ChildContentCtx) -> (i32, i32) {
    let child_metrics = Layouter::build_child_metrics(
        cctx.used_border_box_width,
        HorizontalEdges {
            padding_left: cctx.sides.padding_left,
            padding_right: cctx.sides.padding_right,
            border_left: cctx.sides.border_left,
            border_right: cctx.sides.border_right,
        },
        TopEdges {
            padding_top: cctx.sides.padding_top,
            border_top: cctx.sides.border_top,
        },
        cctx.x,
        cctx.y,
    );
    let (_reflowed, content_height, last_pos_mb, _last_info) =
        layouter.layout_block_children(cctx.key, &child_metrics, cctx.ancestor_applied_at_edge);
    (content_height, last_pos_mb)
}

#[inline]
/// Compute content height and place children for a flex container (single-line MVP).
fn flex_child_content_height(
    layouter: &mut Layouter,
    cctx: ChildContentCtx,
    container_style: &ComputedStyle,
) -> (i32, i32) {
    let ((origin_x, origin_y), direction, axes, container_main_size, main_gap) =
        container_layout_context(cctx, container_style);
    let item_shells = collect_item_shells(layouter, cctx.key);
    let handles = collect_flex_items(&item_shells);
    let (main_items, cross_inputs, baseline_inputs) =
        build_flex_item_inputs(layouter, &handles, direction);
    let (justify, align_items_val, align_content_val, container_cross_size) =
        justify_align_context(container_style, direction, &cross_inputs);
    let container_inputs = FlexContainerInputs {
        direction,
        writing_mode: match container_style.writing_mode {
            CoreWritingMode::HorizontalTb => FlexWritingMode::HorizontalTb,
            CoreWritingMode::VerticalRl => FlexWritingMode::VerticalRl,
            CoreWritingMode::VerticalLr => FlexWritingMode::VerticalLr,
        },
        container_main_size,
        main_gap,
    };
    let cross_gap = match direction {
        FlexDir::Row | FlexDir::RowReverse => container_style.row_gap_percent.map_or_else(
            || container_style.row_gap.max(0.0),
            |percent| (percent * container_cross_size).max(0.0),
        ),
        FlexDir::Column | FlexDir::ColumnReverse => container_style.column_gap_percent.map_or_else(
            || container_style.column_gap.max(0.0),
            |percent| (percent * container_cross_size).max(0.0),
        ),
    };
    let cross_ctx = FlexCrossContext {
        align_items: align_items_val,
        align_content: align_content_val,
        container_cross_size,
        cross_gap,
    };
    let pairs = match container_style.flex_wrap {
        CoreFlexWrap::NoWrap => layout_single_line_with_cross(
            container_inputs,
            justify,
            cross_ctx,
            &main_items,
            FlexCrossAndBaseline {
                cross_inputs: &cross_inputs,
                baseline_inputs: &baseline_inputs,
            },
        ),
        CoreFlexWrap::Wrap => layout_multi_line_with_cross(
            container_inputs,
            justify,
            cross_ctx,
            &main_items,
            FlexCrossAndBaseline {
                cross_inputs: &cross_inputs,
                baseline_inputs: &baseline_inputs,
            },
        ),
    };
    let params = FinalizeParams {
        axes,
        origin_x,
        origin_y,
        pairs: &pairs,
        container_style,
        container_cross_size,
        direction,
        justify,
        align_items: align_items_val,
        container_main_size,
    };
    let clamped_content_height = finalize_flex_container(layouter, cctx, &params);
    (clamped_content_height, 0)
}

#[derive(Copy, Clone)]
/// Parameters required to finalize a flex container after main/cross placement.
struct FinalizeParams<'params> {
    /// Resolved axes (main maps to inline?).
    axes: FlexAxes,
    /// Container padding-box origin X coordinate.
    origin_x: i32,
    /// Container padding-box origin Y coordinate.
    origin_y: i32,
    /// Paired main/cross placements for items.
    pairs: &'params [(FlexPlacement, CrossPlacement)],
    /// Container computed style.
    container_style: &'params ComputedStyle,
    /// Container cross size used for overflow clamping.
    container_cross_size: f32,
    /// Flex direction for abspos static-position solve.
    direction: FlexDir,
    /// Justify-content for abspos static-position solve.
    justify: FlexJustify,
    /// Align-items for abspos static-position solve.
    align_items: FlexAlignItems,
    /// Container main size for percentage resolution.
    container_main_size: f32,
}

#[inline]
/// Write rects, apply overflow hidden clamp, and place abspos children. Returns clamped content height.
fn finalize_flex_container(
    layouter: &mut Layouter,
    cctx: ChildContentCtx,
    params: &FinalizeParams<'_>,
) -> i32 {
    let content_height = write_pairs_and_measure(
        params.axes,
        params.origin_x,
        params.origin_y,
        layouter,
        params.pairs,
    );
    let clamped = clamped_content_height_for_overflow(
        matches!(params.container_style.overflow, CoreOverflow::Hidden),
        content_height,
        params.container_cross_size,
    );
    let abs_ctx = AbsContainerCtx {
        origin_x: params.origin_x,
        origin_y: params.origin_y,
        axes: params.axes,
        container_main_size: params.container_main_size,
        container_cross_size: params.container_cross_size,
        direction: params.direction,
        justify: params.justify,
        align_items: params.align_items,
        writing_mode: match params.container_style.writing_mode {
            CoreWritingMode::HorizontalTb => FlexWritingMode::HorizontalTb,
            CoreWritingMode::VerticalRl => FlexWritingMode::VerticalRl,
            CoreWritingMode::VerticalLr => FlexWritingMode::VerticalLr,
        },
    };
    place_absolute_children(layouter, cctx.key, abs_ctx);
    clamped
}

#[derive(Copy, Clone)]
/// Minimal context for absolute positioning within a flex container's padding box.
struct AbsContainerCtx {
    /// Container padding box origin-x in layout coordinates.
    origin_x: i32,
    /// Container padding box origin-y in layout coordinates.
    origin_y: i32,
    /// Flex axes derived from direction and writing mode.
    axes: FlexAxes,
    /// Container main-axis size used for percentage resolution.
    container_main_size: f32,
    /// Container cross-axis size used for percentage resolution.
    container_cross_size: f32,
    /// Flex direction (needed to compute static position as if sole item).
    direction: FlexDir,
    /// Container justify-content for static position computation.
    justify: FlexJustify,
    /// Container align-items for static position computation.
    align_items: FlexAlignItems,
    /// Writing mode for axis resolution when computing abspos static position and sizes.
    writing_mode: FlexWritingMode,
}

/// Clamp content height when overflow is hidden (minimal overflow contract for flex containers).
#[inline]
const fn clamped_content_height_for_overflow(
    hidden: bool,
    content_height: i32,
    container_cross_size: f32,
) -> i32 {
    if hidden {
        let clamp_to = container_cross_size as i32;
        if content_height > clamp_to {
            clamp_to
        } else {
            content_height
        }
    } else {
        content_height
    }
}

/// Resolve container inline/block sizes from flex axes.
#[inline]
const fn resolve_axis_sizes(ctx: &AbsContainerCtx) -> (f32, f32) {
    let inline_size = if ctx.axes.main_is_inline {
        ctx.container_main_size
    } else {
        ctx.container_cross_size
    };
    let block_size = if ctx.axes.main_is_inline {
        ctx.container_cross_size
    } else {
        ctx.container_main_size
    };
    (inline_size, block_size)
}

/// Resolved offsets: (left, right, top, bottom) in px.
type Offsets = (Option<f32>, Option<f32>, Option<f32>, Option<f32>);

/// Resolve percentage/px offsets against inline/block sizes.
#[inline]
fn resolve_offsets(style: &ComputedStyle, inline_size: f32, block_size: f32) -> Offsets {
    let left_resolved = style
        .left_percent
        .map(|percent| (percent * inline_size).max(0.0))
        .or_else(|| style.left.map(|value| value.max(0.0)));
    let right_resolved = style
        .right_percent
        .map(|percent| (percent * inline_size).max(0.0))
        .or_else(|| style.right.map(|value| value.max(0.0)));
    let top_resolved = style
        .top_percent
        .map(|percent| (percent * block_size).max(0.0))
        .or_else(|| style.top.map(|value| value.max(0.0)));
    let bottom_resolved = style
        .bottom_percent
        .map(|percent| (percent * block_size).max(0.0))
        .or_else(|| style.bottom.map(|value| value.max(0.0)));
    (left_resolved, right_resolved, top_resolved, bottom_resolved)
}

/// Resolve used width/height with auto sizing when both opposite offsets are specified.
#[inline]
fn resolve_used_dimensions(
    style: &ComputedStyle,
    offsets: Offsets,
    sizes: (f32, f32),
) -> (f32, f32) {
    let (left_resolved, right_resolved, top_resolved, bottom_resolved) = offsets;
    let (inline_size, block_size) = sizes;
    let used_width = if style.width.is_none()
        && let (Some(left_px), Some(right_px)) = (left_resolved, right_resolved)
    {
        (inline_size - left_px - right_px).max(0.0)
    } else {
        style.width.unwrap_or(0.0).max(0.0)
    };
    let used_height = if style.height.is_none()
        && let (Some(top_px), Some(bottom_px)) = (top_resolved, bottom_resolved)
    {
        (block_size - top_px - bottom_px).max(0.0)
    } else {
        style.height.unwrap_or(0.0).max(0.0)
    };
    (used_width, used_height)
}

#[inline]
/// Build the single flex item used to compute the abspos static position and return
/// the item, its cross size, and the initial cross-axis margin (for coordinate mapping).
fn build_abspos_item(
    child: NodeKey,
    style: &ComputedStyle,
    ctx: &AbsContainerCtx,
) -> (FlexChild, f32, f32) {
    let (basis_px, cross_px, margin_cross_start) = if ctx.axes.main_is_inline {
        let basis = style.width.unwrap_or(0.0).max(0.0);
        let cross = style.height.unwrap_or(0.0).max(0.0);
        (basis, cross, style.margin.top.max(0.0))
    } else {
        let basis = style.height.unwrap_or(0.0).max(0.0);
        let cross = style.width.unwrap_or(0.0).max(0.0);
        (basis, cross, style.margin.left.max(0.0))
    };
    let item = FlexChild {
        handle: FlexItemRef(child.0),
        flex_basis: basis_px,
        flex_grow: 0.0,
        flex_shrink: 0.0,
        min_main: 0.0,
        max_main: 1e9,
        margin_left: if ctx.axes.main_is_inline {
            style.margin.left.max(0.0)
        } else {
            style.margin.top.max(0.0)
        },
        margin_right: if ctx.axes.main_is_inline {
            style.margin.right.max(0.0)
        } else {
            style.margin.bottom.max(0.0)
        },
        margin_top: if ctx.axes.main_is_inline {
            style.margin.top.max(0.0)
        } else {
            style.margin.left.max(0.0)
        },
        margin_bottom: if ctx.axes.main_is_inline {
            style.margin.bottom.max(0.0)
        } else {
            style.margin.right.max(0.0)
        },
        margin_left_auto: false,
        margin_right_auto: false,
    };
    (item, cross_px, margin_cross_start)
}

/// Build container and cross contexts for the one-line flex solve used by abspos static positioning.
#[inline]
const fn build_abspos_contexts(
    ctx: &AbsContainerCtx,
    inline_size: f32,
    block_size: f32,
) -> (FlexContainerInputs, FlexCrossContext) {
    let container_inputs = FlexContainerInputs {
        direction: ctx.direction,
        writing_mode: ctx.writing_mode,
        container_main_size: if ctx.axes.main_is_inline {
            inline_size
        } else {
            block_size
        },
        main_gap: 0.0,
    };
    let cross_ctx = FlexCrossContext {
        align_items: ctx.align_items,
        align_content: FlexAlignContent::Start,
        container_cross_size: if ctx.axes.main_is_inline {
            block_size
        } else {
            inline_size
        },
        cross_gap: 0.0,
    };
    (container_inputs, cross_ctx)
}

#[inline]
/// Compute static position as if the child were the sole flex item.
fn static_position_xy(
    child: NodeKey,
    style: &ComputedStyle,
    ctx: &AbsContainerCtx,
    inline_size: f32,
    block_size: f32,
) -> (f32, f32) {
    let (item, cross_px, margin_cross_start) = build_abspos_item(child, style, ctx);
    let (container_inputs, cross_ctx) = build_abspos_contexts(ctx, inline_size, block_size);
    let cross_inputs = [(cross_px, 0.0, 1e9)];
    let cab = FlexCrossAndBaseline {
        cross_inputs: &cross_inputs,
        baseline_inputs: &[None],
    };
    let pairs =
        layout_single_line_with_cross(container_inputs, ctx.justify, cross_ctx, &[item], cab);
    pairs
        .first()
        .map_or((ctx.origin_x as f32, ctx.origin_y as f32), |first| {
            let (place, cross) = *first;
            let x = if ctx.axes.main_is_inline {
                ctx.origin_x as f32 + place.main_offset
            } else {
                ctx.origin_x as f32 + cross.cross_offset + margin_cross_start
            };
            let y = if ctx.axes.main_is_inline {
                ctx.origin_y as f32 + cross.cross_offset + margin_cross_start
            } else {
                ctx.origin_y as f32 + place.main_offset
            };
            (x, y)
        })
}

#[inline]
/// Compute the used absolute rectangle for a positioned child, resolving percentage offsets and
/// supporting auto sizing when both opposite offsets are specified.
fn compute_abs_rect(child: NodeKey, style: &ComputedStyle, ctx: AbsContainerCtx) -> LayoutRect {
    let sizes = resolve_axis_sizes(&ctx);
    let (inline_size, block_size) = sizes;
    let offsets = resolve_offsets(style, inline_size, block_size);
    let (used_width, used_height) = resolve_used_dimensions(style, offsets, sizes);
    let (left_resolved, right_resolved, top_resolved, bottom_resolved) = offsets;

    // Compute x/y: left takes precedence; otherwise resolve from right; otherwise use static position.
    let (static_x, static_y) = static_position_xy(child, style, &ctx, inline_size, block_size);
    let x = left_resolved.map_or_else(
        || {
            right_resolved.map_or(static_x, |right_px| {
                (ctx.origin_x as f32) + (inline_size - right_px - used_width).max(0.0)
            })
        },
        |left_px| (ctx.origin_x as f32) + left_px,
    );
    let y = top_resolved.map_or_else(
        || {
            bottom_resolved.map_or(static_y, |bottom_px| {
                (ctx.origin_y as f32) + (block_size - bottom_px - used_height).max(0.0)
            })
        },
        |top_px| (ctx.origin_y as f32) + top_px,
    );
    LayoutRect {
        x,
        y,
        width: used_width,
        height: used_height,
    }
}

#[inline]
/// Place absolutely positioned children relative to the container's padding box origin.
fn place_absolute_children(layouter: &mut Layouter, parent: NodeKey, ctx: AbsContainerCtx) {
    let children = layouter.children.get(&parent).cloned().unwrap_or_default();
    for child in children {
        let style = layouter
            .computed_styles
            .get(&child)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        if !matches!(style.position, CorePosition::Absolute) {
            continue;
        }
        let rect = compute_abs_rect(child, &style, ctx);
        layouter.rects.insert(child, rect);
    }
}

#[inline]
/// Build `FlexChild` inputs and cross constraints for the given item list.
fn build_flex_item_inputs(
    layouter: &Layouter,
    items: &[FlexItem],
    direction: FlexDir,
) -> FlexInputsTriple {
    let mut main_items: Vec<FlexChild> = Vec::with_capacity(items.len());
    let mut cross_inputs: Vec<CrossTriplet> = Vec::with_capacity(items.len());
    let mut baseline_inputs: BaselineVec = Vec::with_capacity(items.len());
    for item in items {
        let key = NodeKey(item.handle.0);
        let style_item = layouter
            .computed_styles
            .get(&key)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let basis_opt = style_item.flex_basis.or(match direction {
            FlexDir::Row | FlexDir::RowReverse => style_item.width,
            FlexDir::Column | FlexDir::ColumnReverse => style_item.height,
        });
        let flex_basis = basis_opt.unwrap_or(0.0).max(0.0);
        let (min_main, mut max_main) = match direction {
            FlexDir::Row | FlexDir::RowReverse => (
                style_item.min_width.unwrap_or(0.0).max(0.0),
                style_item.max_width.unwrap_or(1_000_000.0).max(0.0),
            ),
            FlexDir::Column | FlexDir::ColumnReverse => (
                style_item.min_height.unwrap_or(0.0).max(0.0),
                style_item.max_height.unwrap_or(1_000_000.0).max(0.0),
            ),
        };
        if max_main < min_main {
            max_main = min_main;
        }
        main_items.push(FlexChild {
            handle: item.handle,
            flex_basis,
            flex_grow: style_item.flex_grow.max(0.0),
            flex_shrink: style_item.flex_shrink.max(0.0),
            min_main,
            max_main,
            margin_left: style_item.margin.left.max(0.0),
            margin_right: style_item.margin.right.max(0.0),
            margin_top: style_item.margin.top.max(0.0),
            margin_bottom: style_item.margin.bottom.max(0.0),
            margin_left_auto: false,
            margin_right_auto: false,
        });
        let (cross, min_c, mut max_c) = match direction {
            FlexDir::Row | FlexDir::RowReverse => (
                style_item.height.unwrap_or(0.0).max(0.0),
                style_item.min_height.unwrap_or(0.0).max(0.0),
                style_item.max_height.unwrap_or(1_000_000.0).max(0.0),
            ),
            FlexDir::Column | FlexDir::ColumnReverse => (
                style_item.width.unwrap_or(0.0).max(0.0),
                style_item.min_width.unwrap_or(0.0).max(0.0),
                style_item.max_width.unwrap_or(1_000_000.0).max(0.0),
            ),
        };
        if max_c < min_c {
            max_c = min_c;
        }
        cross_inputs.push((cross, min_c, max_c));
        // Baseline metrics [Approximation → Improved]:
        // 1) Prefer real baselines from inline/text engine when available.
        // 2) Fallback heuristic: first = line-height clamped to cross; last = cross - first.
        if let Some((first_real, last_real)) = try_inline_baselines(layouter, key) {
            baseline_inputs.push(Some((first_real.max(0.0), last_real.max(0.0))));
        } else {
            let line_h_px = style_item
                .line_height
                .unwrap_or_else(|| default_line_height_px(&style_item) as f32)
                .max(0.0);
            let first_baseline = line_h_px.min(cross).max(0.0);
            let last_baseline = (cross - first_baseline).max(0.0).min(cross);
            baseline_inputs.push(Some((first_baseline, last_baseline)));
        }
    }
    (main_items, cross_inputs, baseline_inputs)
}

#[inline]
/// Resolve justify/align context and container cross size from style and item inputs.
fn justify_align_context(
    container_style: &ComputedStyle,
    direction: FlexDir,
    cross_inputs: &[CrossTriplet],
) -> (FlexJustify, FlexAlignItems, FlexAlignContent, f32) {
    let justify = match container_style.justify_content {
        CoreJustify::Center => FlexJustify::Center,
        CoreJustify::FlexEnd => FlexJustify::End,
        CoreJustify::SpaceBetween => FlexJustify::SpaceBetween,
        CoreJustify::SpaceAround => FlexJustify::SpaceAround,
        CoreJustify::SpaceEvenly => FlexJustify::SpaceEvenly,
        CoreJustify::FlexStart => FlexJustify::Start,
    };
    let align_items_val = match container_style.align_items {
        CoreAlignItems::Center => FlexAlignItems::Center,
        CoreAlignItems::FlexEnd => FlexAlignItems::FlexEnd,
        CoreAlignItems::FlexStart => FlexAlignItems::FlexStart,
        CoreAlignItems::Stretch => FlexAlignItems::Stretch,
    };
    let align_content_val = match container_style.align_content {
        CoreAlignContent::Center => FlexAlignContent::Center,
        CoreAlignContent::FlexEnd => FlexAlignContent::End,
        CoreAlignContent::SpaceBetween => FlexAlignContent::SpaceBetween,
        CoreAlignContent::SpaceAround => FlexAlignContent::SpaceAround,
        CoreAlignContent::SpaceEvenly => FlexAlignContent::SpaceEvenly,
        CoreAlignContent::Stretch => FlexAlignContent::Stretch,
        CoreAlignContent::FlexStart => FlexAlignContent::Start,
    };
    let mut container_cross_size: f32 = match direction {
        FlexDir::Row | FlexDir::RowReverse => container_style.height.unwrap_or(0.0).max(0.0),
        FlexDir::Column | FlexDir::ColumnReverse => container_style.width.unwrap_or(0.0).max(0.0),
    };
    if container_cross_size <= 0.0 {
        container_cross_size = cross_inputs
            .iter()
            .copied()
            .map(|triple| {
                let (cross_val, _min_cross, _max_cross) = triple;
                cross_val
            })
            .fold(0.0f32, f32::max);
    }
    (
        justify,
        align_items_val,
        align_content_val,
        container_cross_size,
    )
}

#[inline]
/// Write item rectangles and return the computed content height for the container.
fn write_pairs_and_measure(
    axes: FlexAxes,
    origin_x: i32,
    origin_y: i32,
    layouter: &mut Layouter,
    pairs: &[(FlexPlacement, CrossPlacement)],
) -> i32 {
    let mut max_main_extent: f32 = 0.0;
    let mut max_cross_extent: f32 = 0.0;
    for (place, cross) in pairs.iter().copied() {
        let key = NodeKey(place.handle.0);
        // Include per-item margins only along the cross axis for row direction (top)
        // or along the inline axis for column direction (left). Main-axis margins
        // are already accounted for in FlexPlacement.main_offset.
        let style = layouter
            .computed_styles
            .get(&key)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let margin_left = style.margin.left.max(0.0);
        let margin_top = style.margin.top.max(0.0);
        let pos_x: f32 = if axes.main_is_inline {
            origin_x as f32 + place.main_offset
        } else {
            origin_x as f32 + cross.cross_offset + margin_left
        };
        let pos_y: f32 = if axes.main_is_inline {
            origin_y as f32 + cross.cross_offset + margin_top
        } else {
            origin_y as f32 + place.main_offset
        };
        let width_px: f32 = if axes.main_is_inline {
            place.main_size
        } else {
            cross.cross_size
        };
        let height_px: f32 = if axes.main_is_inline {
            cross.cross_size
        } else {
            place.main_size
        };
        layouter.rects.insert(
            key,
            LayoutRect {
                x: pos_x,
                y: pos_y,
                width: width_px,
                height: height_px,
            },
        );
        max_main_extent = max_main_extent.max(place.main_offset + place.main_size);
        max_cross_extent = max_cross_extent.max(cross.cross_offset + cross.cross_size);
    }
    if axes.main_is_inline {
        max_cross_extent as i32
    } else {
        max_main_extent as i32
    }
}
