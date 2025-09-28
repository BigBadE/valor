//! Spec: CSS 2.2 §10.6.3 The height of blocks
//! Root height and used height computations.

use crate::LayoutRect;
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
    JustifyContent as CoreJustify, Position as CorePosition,
};
use css_text::default_line_height_px;
use js::NodeKey;
use std::collections::HashSet;

use crate::LayoutNodeKind;

use crate::chapter8::part_8_3_1_collapsing_margins as cm83;
use crate::chapter9::part_9_4_1_block_formatting_context::establishes_block_formatting_context;
use crate::{
    ChildContentCtx, HeightExtras, HeightsAndMargins, HeightsCtx, HorizontalEdges, Layouter,
    RootHeightsCtx, TopEdges,
};

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
    let writing_mode = FlexWritingMode::HorizontalTb;
    let axes = flex_resolve_axes(direction, writing_mode);
    let container_main_size: f32 = if axes.main_is_inline {
        // Main axis maps to inline axis: use container inline size (width in HorizontalTb)
        metrics.container_width as f32
    } else {
        // Main axis maps to block axis: use computed height when specified; otherwise treat as unbounded
        container_style.height.unwrap_or(1_000_000.0).max(0.0)
    };
    let main_gap = match container_style.flex_direction {
        CoreFlexDirection::Column => container_style.row_gap,
        CoreFlexDirection::Row => container_style.column_gap,
    }
    .max(0.0);
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
        writing_mode: FlexWritingMode::HorizontalTb,
        container_main_size,
        main_gap,
    };
    let cross_gap = match direction {
        FlexDir::Row | FlexDir::RowReverse => container_style.row_gap,
        FlexDir::Column | FlexDir::ColumnReverse => container_style.column_gap,
    }
    .max(0.0);
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
    let content_height = write_pairs_and_measure(axes, origin_x, origin_y, layouter, &pairs);
    (content_height, 0)
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
        let (min_main, max_main) = match direction {
            FlexDir::Row | FlexDir::RowReverse => (
                style_item.min_width.unwrap_or(0.0).max(0.0),
                style_item.max_width.unwrap_or(1_000_000.0).max(0.0),
            ),
            FlexDir::Column | FlexDir::ColumnReverse => (
                style_item.min_height.unwrap_or(0.0).max(0.0),
                style_item.max_height.unwrap_or(1_000_000.0).max(0.0),
            ),
        };
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
        let (cross, min_c, max_c) = match direction {
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
        cross_inputs.push((cross, min_c, max_c));
        // Baseline metrics [Approximation → Improved]:
        // Use the computed or default line-height as a proxy for first baseline when available.
        // Last baseline stays at 0 for MVP until inline layout provides real metrics.
        let line_h_px = style_item
            .line_height
            .unwrap_or_else(|| default_line_height_px(&style_item) as f32)
            .max(0.0);
        let first_baseline = line_h_px.min(cross).max(0.0);
        let last_baseline = 0.0f32;
        baseline_inputs.push(Some((first_baseline, last_baseline)));
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
