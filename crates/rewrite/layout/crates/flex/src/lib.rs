//! Flexbox layout implementation.
//!
//! This module provides flexbox layout computation using the Dispatcher trait
//! to query sizes and offsets without circular dependencies.

use rewrite_core::ScopedDb;
use rewrite_css::{
    AlignItemsQuery, AlignSelfQuery, CssKeyword, CssValue, FlexDirectionQuery, FlexWrapQuery,
    JustifyContentQuery, Subpixels,
};
use rewrite_css::{
    BlockMarker as CssBlockMarker, EndMarker, InlineMarker as CssInlineMarker, StartMarker,
};
use rewrite_css_dimensional::{
    BorderWidthQuery, ColumnGapQuery, FlexGrowQuery, FlexShrinkQuery, PaddingQuery, RowGapQuery,
};
use rewrite_layout_offset_impl::OffsetMode;
use rewrite_layout_size_impl::{
    ConstrainedMarker, DispatchedSizeQuery, FlexSizeDispatcher, SizeDispatcher, SizeMode,
};
use rewrite_layout_util::{
    Axis, BlockMarker as LayoutBlockMarker, Dispatcher, InlineMarker as LayoutInlineMarker,
};

/// Flex size dispatcher implementation.
pub struct FlexSize;

impl FlexSizeDispatcher for FlexSize {
    fn compute_flex_size<D>(scoped: &mut ScopedDb, axis: Axis, mode: SizeMode) -> Subpixels
    where
        D: SizeDispatcher + 'static,
    {
        compute_flex_size_impl::<D>(scoped, axis, mode)
    }
}

/// Internal implementation of flex size computation.
fn compute_flex_size_impl<D>(scoped: &mut ScopedDb, axis: Axis, mode: SizeMode) -> Subpixels
where
    D: SizeDispatcher + 'static,
{
    let flex_direction = scoped.query::<FlexDirectionQuery>();

    match (axis, mode) {
        (Axis::Block, SizeMode::Constrained) => {
            if is_main_axis_block(&flex_direction) {
                compute_main_axis_size::<D>(scoped, Axis::Block)
            } else {
                compute_cross_axis_size::<D>(scoped, Axis::Block)
            }
        }
        (Axis::Inline, SizeMode::Constrained) => {
            if is_main_axis_inline(&flex_direction) {
                compute_main_axis_size::<D>(scoped, Axis::Inline)
            } else {
                compute_cross_axis_size::<D>(scoped, Axis::Inline)
            }
        }
        (Axis::Inline, SizeMode::Intrinsic) => {
            // Intrinsic inline size: use parent's available space minus padding/border
            if let Some(parent_id) = scoped.parent_id() {
                let mut parent_scoped = scoped.scoped_to(parent_id);
                let parent_size = D::query(&mut parent_scoped, Axis::Inline, SizeMode::Constrained);

                // Subtract parent's padding and border
                let padding_start =
                    parent_scoped.query::<PaddingQuery<CssInlineMarker, StartMarker>>();
                let padding_end = parent_scoped.query::<PaddingQuery<CssInlineMarker, EndMarker>>();
                let border_start =
                    parent_scoped.query::<BorderWidthQuery<CssInlineMarker, StartMarker>>();
                let border_end =
                    parent_scoped.query::<BorderWidthQuery<CssInlineMarker, EndMarker>>();

                parent_size - padding_start - padding_end - border_start - border_end
            } else {
                0
            }
        }
        (Axis::Block, SizeMode::Intrinsic) => {
            // Intrinsic block size: sum of children
            compute_cross_axis_size::<D>(scoped, Axis::Block)
        }
    }
}

/// Compute the offset of a flex item.
pub fn compute_flex_offset<OffsetD, SizeD>(
    scoped: &mut ScopedDb,
    axis: Axis,
    _mode: OffsetMode,
) -> Subpixels
where
    OffsetD: Dispatcher<(Axis, OffsetMode), Returns = Subpixels>,
    SizeD: SizeDispatcher + 'static,
{
    let flex_direction = scoped.parent::<FlexDirectionQuery>();

    match axis {
        Axis::Block => {
            if is_main_axis_block(&flex_direction) {
                compute_main_axis_offset::<OffsetD, SizeD>(scoped, Axis::Block)
            } else {
                compute_cross_axis_offset::<OffsetD, SizeD>(scoped, Axis::Block)
            }
        }
        Axis::Inline => {
            if is_main_axis_inline(&flex_direction) {
                compute_main_axis_offset::<OffsetD, SizeD>(scoped, Axis::Inline)
            } else {
                compute_cross_axis_offset::<OffsetD, SizeD>(scoped, Axis::Inline)
            }
        }
    }
}

// ============================================================================
// Main Axis Offset
// ============================================================================

fn compute_main_axis_offset<OffsetD, SizeD>(scoped: &mut ScopedDb, axis: Axis) -> Subpixels
where
    OffsetD: Dispatcher<(Axis, OffsetMode), Returns = Subpixels>,
    SizeD: SizeDispatcher + 'static,
{
    // Get parent's offset and add padding
    let parent_offset = if let Some(parent_id) = scoped.parent_id() {
        let mut parent_scoped = scoped.scoped_to(parent_id);
        OffsetD::query(&mut parent_scoped, (axis, OffsetMode::Static))
    } else {
        0
    };

    let parent_padding_start = match axis {
        Axis::Block => scoped.parent::<PaddingQuery<CssBlockMarker, StartMarker>>(),
        Axis::Inline => scoped.parent::<PaddingQuery<CssInlineMarker, StartMarker>>(),
    };

    let parent_start = parent_offset + parent_padding_start;

    // Get gap between items
    let gap = match axis {
        Axis::Block => scoped.parent::<RowGapQuery>(),
        Axis::Inline => scoped.parent::<ColumnGapQuery>(),
    };

    // Sum sizes of previous siblings using actual size queries
    let prev_sizes: Subpixels = match axis {
        Axis::Block => scoped
            .prev_siblings::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, SizeD, FlexSize>>()
            .sum(),
        Axis::Inline => scoped
            .prev_siblings::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, SizeD, FlexSize>>(
            )
            .sum(),
    };

    let sibling_index = scoped.prev_siblings_count();
    let gaps_before = gap * sibling_index as i32;
    let base_offset = parent_start + prev_sizes + gaps_before;

    // Apply justify-content
    let justify = scoped.parent::<JustifyContentQuery>();
    apply_justify_content::<OffsetD, SizeD>(scoped, base_offset, &justify, sibling_index, axis)
}

fn apply_justify_content<OffsetD, SizeD>(
    scoped: &mut ScopedDb,
    base_offset: Subpixels,
    justify: &CssValue,
    sibling_index: usize,
    axis: Axis,
) -> Subpixels
where
    OffsetD: Dispatcher<(Axis, OffsetMode), Returns = Subpixels>,
    SizeD: SizeDispatcher + 'static,
{
    match justify {
        CssValue::Keyword(CssKeyword::FlexStart) | CssValue::Keyword(CssKeyword::Start) => {
            base_offset
        }
        CssValue::Keyword(CssKeyword::FlexEnd) | CssValue::Keyword(CssKeyword::End) => {
            let free_space = compute_free_space::<SizeD>(scoped, axis);
            base_offset + free_space.max(0)
        }
        CssValue::Keyword(CssKeyword::Center) => {
            let free_space = compute_free_space::<SizeD>(scoped, axis);
            base_offset + (free_space.max(0) / 2)
        }
        CssValue::Keyword(CssKeyword::SpaceBetween) => {
            let items_count = scoped.parent_children_count();
            if items_count <= 1 {
                return base_offset;
            }
            let free_space = compute_free_space::<SizeD>(scoped, axis);
            let spacing = free_space.max(0) / (items_count as i32 - 1);
            base_offset + spacing * sibling_index as i32
        }
        CssValue::Keyword(CssKeyword::SpaceAround) => {
            let items_count = scoped.parent_children_count();
            if items_count == 0 {
                return base_offset;
            }
            let free_space = compute_free_space::<SizeD>(scoped, axis);
            let spacing = free_space.max(0) / items_count as i32;
            base_offset + spacing * sibling_index as i32 + spacing / 2
        }
        CssValue::Keyword(CssKeyword::SpaceEvenly) => {
            let items_count = scoped.parent_children_count();
            if items_count == 0 {
                return base_offset;
            }
            let free_space = compute_free_space::<SizeD>(scoped, axis);
            let spacing = free_space.max(0) / (items_count as i32 + 1);
            base_offset + spacing * (sibling_index as i32 + 1)
        }
        _ => base_offset,
    }
}

fn compute_free_space<SizeD>(scoped: &mut ScopedDb, axis: Axis) -> Subpixels
where
    SizeD: SizeDispatcher + 'static,
{
    // Get parent size
    let parent_size = match axis {
        Axis::Block => {
            if let Some(parent_id) = scoped.parent_id() {
                let mut parent_scoped = scoped.scoped_to(parent_id);
                parent_scoped
                    .query::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, SizeD, FlexSize>>()
            } else {
                0
            }
        }
        Axis::Inline => {
            if let Some(parent_id) = scoped.parent_id() {
                let mut parent_scoped = scoped.scoped_to(parent_id);
                parent_scoped
                    .query::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, SizeD, FlexSize>>(
                    )
            } else {
                0
            }
        }
    };

    // Get total size of all children
    let children_size: Subpixels = match axis {
        Axis::Block => {
            if let Some(parent_id) = scoped.parent_id() {
                let mut parent_scoped = scoped.scoped_to(parent_id);
                parent_scoped
                    .children::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, SizeD, FlexSize>>()
                    .sum()
            } else {
                0
            }
        }
        Axis::Inline => {
            if let Some(parent_id) = scoped.parent_id() {
                let mut parent_scoped = scoped.scoped_to(parent_id);
                parent_scoped
                    .children::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, SizeD, FlexSize>>()
                    .sum()
            } else {
                0
            }
        }
    };

    // Get gap
    let gap = match axis {
        Axis::Block => scoped.parent::<RowGapQuery>(),
        Axis::Inline => scoped.parent::<ColumnGapQuery>(),
    };

    let children_count = scoped.parent_children_count();
    let total_gaps = if children_count > 1 {
        gap * (children_count as i32 - 1)
    } else {
        0
    };

    parent_size - children_size - total_gaps
}

// ============================================================================
// Cross Axis Offset
// ============================================================================

fn compute_cross_axis_offset<OffsetD, SizeD>(scoped: &mut ScopedDb, axis: Axis) -> Subpixels
where
    OffsetD: Dispatcher<(Axis, OffsetMode), Returns = Subpixels>,
    SizeD: SizeDispatcher + 'static,
{
    let parent_offset = if let Some(parent_id) = scoped.parent_id() {
        let mut parent_scoped = scoped.scoped_to(parent_id);
        OffsetD::query(&mut parent_scoped, (axis, OffsetMode::Static))
    } else {
        0
    };

    let parent_padding_start = match axis {
        Axis::Block => scoped.parent::<PaddingQuery<CssBlockMarker, StartMarker>>(),
        Axis::Inline => scoped.parent::<PaddingQuery<CssInlineMarker, StartMarker>>(),
    };

    let parent_start = parent_offset + parent_padding_start;

    // Get alignment
    let align_self = scoped.query::<AlignSelfQuery>();
    let align = if align_self != CssValue::Keyword(CssKeyword::Auto) {
        align_self
    } else {
        scoped.parent::<AlignItemsQuery>()
    };

    // Get parent and item sizes
    let parent_size = match axis {
        Axis::Block => {
            if let Some(parent_id) = scoped.parent_id() {
                let mut parent_scoped = scoped.scoped_to(parent_id);
                parent_scoped
                    .query::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, SizeD, FlexSize>>()
            } else {
                0
            }
        }
        Axis::Inline => {
            if let Some(parent_id) = scoped.parent_id() {
                let mut parent_scoped = scoped.scoped_to(parent_id);
                parent_scoped
                    .query::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, SizeD, FlexSize>>(
                    )
            } else {
                0
            }
        }
    };

    let item_size = match axis {
        Axis::Block => {
            scoped.query::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, SizeD, FlexSize>>()
        }
        Axis::Inline => {
            scoped.query::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, SizeD, FlexSize>>()
        }
    };

    match align {
        CssValue::Keyword(CssKeyword::FlexStart) | CssValue::Keyword(CssKeyword::Start) => {
            parent_start
        }
        CssValue::Keyword(CssKeyword::FlexEnd) | CssValue::Keyword(CssKeyword::End) => {
            let parent_padding_end = match axis {
                Axis::Block => scoped.parent::<PaddingQuery<CssBlockMarker, EndMarker>>(),
                Axis::Inline => scoped.parent::<PaddingQuery<CssInlineMarker, EndMarker>>(),
            };
            parent_start + parent_size - item_size - parent_padding_end
        }
        CssValue::Keyword(CssKeyword::Center) => parent_start + (parent_size - item_size) / 2,
        CssValue::Keyword(CssKeyword::Stretch) => {
            // Stretch handled in sizing, position at start
            parent_start
        }
        CssValue::Keyword(CssKeyword::Baseline) => {
            // Baseline alignment - approximate as flex-start
            parent_start
        }
        _ => parent_start,
    }
}

// ============================================================================
// Main Axis Size
// ============================================================================

fn compute_main_axis_size<D>(scoped: &mut ScopedDb, axis: Axis) -> Subpixels
where
    D: SizeDispatcher + 'static,
{
    let children_count = scoped.children_count();
    if children_count == 0 {
        return 0;
    }

    // Get container size
    let container_size = D::query(scoped, axis, SizeMode::Constrained);

    // Collect children base sizes
    let base_sizes: Vec<Subpixels> = match axis {
        Axis::Block => scoped
            .children::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, D, FlexSize>>()
            .collect(),
        Axis::Inline => scoped
            .children::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, D, FlexSize>>()
            .collect(),
    };

    let total_base: Subpixels = base_sizes.iter().sum();

    // Get gap
    let gap = match axis {
        Axis::Block => scoped.query::<RowGapQuery>(),
        Axis::Inline => scoped.query::<ColumnGapQuery>(),
    };
    let total_gaps = if children_count > 1 {
        gap * (children_count as i32 - 1)
    } else {
        0
    };

    // Calculate free space
    let free_space = container_size - total_base - total_gaps;

    if free_space > 0 {
        // Apply flex-grow
        apply_flex_grow(scoped, base_sizes, free_space, total_gaps)
    } else if free_space < 0 {
        // Apply flex-shrink
        apply_flex_shrink(scoped, base_sizes, free_space.abs(), total_gaps)
    } else {
        total_base + total_gaps
    }
}

fn apply_flex_grow(
    scoped: &mut ScopedDb,
    base_sizes: Vec<Subpixels>,
    free_space: Subpixels,
    total_gaps: Subpixels,
) -> Subpixels {
    let grow_factors: Vec<Subpixels> = scoped.children::<FlexGrowQuery>().collect();
    let total_grow: i64 = grow_factors.iter().map(|&x| x as i64).sum();

    if total_grow <= 0 {
        return base_sizes.iter().sum::<Subpixels>() + total_gaps;
    }

    // Distribute free space proportionally using fixed-point arithmetic
    let final_sizes: Vec<Subpixels> = base_sizes
        .iter()
        .zip(grow_factors.iter())
        .map(|(base, &grow)| {
            if grow > 0 {
                // Calculate: free_space * (grow / total_grow)
                let extra = ((free_space as i64 * grow as i64) / total_grow) as i32;
                base + extra
            } else {
                *base
            }
        })
        .collect();

    final_sizes.iter().sum::<Subpixels>() + total_gaps
}

fn apply_flex_shrink(
    scoped: &mut ScopedDb,
    base_sizes: Vec<Subpixels>,
    deficit: Subpixels,
    total_gaps: Subpixels,
) -> Subpixels {
    let shrink_factors: Vec<Subpixels> = scoped.children::<FlexShrinkQuery>().collect();

    // Calculate scaled shrink factors (shrink * base_size)
    let scaled_shrinks: Vec<i64> = shrink_factors
        .iter()
        .zip(base_sizes.iter())
        .map(|(&shrink, &base)| (shrink as i64 * base as i64) / 64) // Divide by 64 to normalize fixed-point
        .collect();

    let total_scaled: i64 = scaled_shrinks.iter().sum();

    if total_scaled <= 0 {
        return base_sizes.iter().sum::<Subpixels>() + total_gaps;
    }

    // Shrink each item proportionally
    let final_sizes: Vec<Subpixels> = base_sizes
        .iter()
        .zip(scaled_shrinks.iter())
        .map(|(&base, &scaled)| {
            if scaled > 0 {
                let reduction = ((deficit as i64 * scaled) / total_scaled) as i32;
                (base - reduction).max(0)
            } else {
                base
            }
        })
        .collect();

    final_sizes.iter().sum::<Subpixels>() + total_gaps
}

// ============================================================================
// Cross Axis Size
// ============================================================================

fn compute_cross_axis_size<D>(scoped: &mut ScopedDb, axis: Axis) -> Subpixels
where
    D: SizeDispatcher + 'static,
{
    let flex_wrap = scoped.query::<FlexWrapQuery>();

    match flex_wrap {
        CssValue::Keyword(CssKeyword::Nowrap) => {
            // Single line: maximum child size
            let max_size = match axis {
                Axis::Block => scoped
                    .children::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, D, FlexSize>>()
                    .max()
                    .unwrap_or(0),
                Axis::Inline => scoped
                    .children::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, D, FlexSize>>()
                    .max()
                    .unwrap_or(0),
            };
            max_size
        }
        CssValue::Keyword(CssKeyword::Wrap) | CssValue::Keyword(CssKeyword::WrapReverse) => {
            // Multi-line: estimate line count and multiply by max item size
            // This is simplified - full implementation would break into actual lines
            let children_count = scoped.children_count();
            if children_count == 0 {
                return 0;
            }

            let max_size = match axis {
                Axis::Block => scoped
                    .children::<DispatchedSizeQuery<LayoutBlockMarker, ConstrainedMarker, D, FlexSize>>()
                    .max()
                    .unwrap_or(0),
                Axis::Inline => scoped
                    .children::<DispatchedSizeQuery<LayoutInlineMarker, ConstrainedMarker, D, FlexSize>>()
                    .max()
                    .unwrap_or(0),
            };

            // Estimate 3 items per line
            let estimated_lines = (children_count + 2) / 3;
            max_size * estimated_lines as i32
        }
        _ => 0,
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn is_main_axis_block(flex_direction: &CssValue) -> bool {
    matches!(
        flex_direction,
        CssValue::Keyword(CssKeyword::Column) | CssValue::Keyword(CssKeyword::ColumnReverse)
    )
}

fn is_main_axis_inline(flex_direction: &CssValue) -> bool {
    matches!(
        flex_direction,
        CssValue::Keyword(CssKeyword::Row) | CssValue::Keyword(CssKeyword::RowReverse)
    )
}
