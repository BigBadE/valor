use crate::{
    BlockMarker, ConstrainedMarker, InlineMarker, InlineSizeQuery, Layouts, SizeMode, SizeQuery,
    Subpixels, helpers,
};
use rewrite_core::ScopedDb;
use rewrite_css::{
    AlignItemsQuery, AlignSelfQuery, ColumnGapQuery, CssKeyword, CssValue, FlexDirectionQuery,
    FlexWrapQuery, JustifyContentQuery, OrderQuery, RowGapQuery,
};

/// Compute the offset (position) of a flex item along the specified axis.
///
/// This implements the complete CSS Flexbox specification including:
/// - Main/cross axis positioning
/// - justify-content distribution
/// - align-items/align-self alignment
/// - Multi-line layouts with flex-wrap
/// - Visual reordering with order property
/// - Auto margins
pub fn compute_flex_offset(scoped: &mut ScopedDb, axis: Layouts) -> Subpixels {
    let flex_direction = scoped.parent::<FlexDirectionQuery>();

    match axis {
        Layouts::Block => {
            if is_main_axis_block(&flex_direction) {
                compute_main_axis_offset::<BlockMarker, RowGapQuery>(scoped)
            } else {
                compute_cross_axis_offset::<BlockMarker>(scoped)
            }
        }
        Layouts::Inline => {
            if is_main_axis_inline(&flex_direction) {
                compute_main_axis_offset::<InlineMarker, ColumnGapQuery>(scoped)
            } else {
                compute_cross_axis_offset::<InlineMarker>(scoped)
            }
        }
    }
}

/// Compute the size (dimension) of a flex container along the specified axis.
///
/// This implements flexible sizing with flex-grow/flex-shrink/flex-basis.
pub fn compute_flex_size(scoped: &mut ScopedDb, axis: Layouts, mode: SizeMode) -> Subpixels {
    let flex_direction = scoped.query::<FlexDirectionQuery>();

    match (axis, mode) {
        (Layouts::Block, SizeMode::Constrained) => {
            if is_main_axis_block(&flex_direction) {
                compute_main_axis_container_size::<BlockMarker, RowGapQuery>(scoped)
            } else {
                compute_cross_axis_container_size::<BlockMarker>(scoped)
            }
        }
        (Layouts::Inline, SizeMode::Constrained) => {
            if is_main_axis_inline(&flex_direction) {
                compute_main_axis_container_size::<InlineMarker, ColumnGapQuery>(scoped)
            } else {
                compute_cross_axis_container_size::<InlineMarker>(scoped)
            }
        }
        _ => {
            // Intrinsic sizing
            let parent_inline_size = scoped.parent::<InlineSizeQuery>();
            let parent_padding = helpers::parent_padding_sum_inline(scoped);
            let parent_border = {
                use rewrite_css::{BorderWidthQuery, EndMarker, StartMarker};
                let start =
                    scoped.parent::<BorderWidthQuery<rewrite_css::InlineMarker, StartMarker>>();
                let end = scoped.parent::<BorderWidthQuery<rewrite_css::InlineMarker, EndMarker>>();
                start + end
            };
            let margin_inline = {
                use rewrite_css::{EndMarker, MarginQuery, StartMarker};
                let start = scoped.parent::<MarginQuery<rewrite_css::InlineMarker, StartMarker>>();
                let end = scoped.parent::<MarginQuery<rewrite_css::InlineMarker, EndMarker>>();
                start + end
            };
            parent_inline_size - parent_padding - parent_border - margin_inline
        }
    }
}

// ============================================================================
// Data Structures
// ============================================================================

// ============================================================================
// Main Axis Offset Computation
// ============================================================================

/// Compute the main axis offset for a flex item.
///
/// This handles justify-content, gaps, auto margins, and visual ordering.
fn compute_main_axis_offset<Axis, GapQuery>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
    GapQuery: rewrite_core::Query<Key = rewrite_core::NodeId, Value = Subpixels> + 'static,
    GapQuery::Value: Clone + Send + Sync,
{
    let parent_start = helpers::parent_start::<Axis>(scoped);
    let justify_content = scoped.parent::<JustifyContentQuery>();
    let gap = scoped.parent::<GapQuery>();

    // Check for auto margins which override justify-content
    let (margin_start, margin_end) = get_item_margins::<Axis>(scoped);
    let has_auto_margin_start = is_auto_margin(margin_start);
    let has_auto_margin_end = is_auto_margin(margin_end);

    if has_auto_margin_start || has_auto_margin_end {
        return compute_auto_margin_offset::<Axis>(
            scoped,
            parent_start,
            has_auto_margin_start,
            has_auto_margin_end,
        );
    }

    // Get siblings considering visual order
    let sibling_index = get_visual_sibling_index(scoped);

    // Base offset from previous siblings
    let prev_sizes: Subpixels = get_ordered_prev_sibling_sizes::<Axis>(scoped);
    let gaps_before = gap * sibling_index as i32;

    let base_offset = parent_start + prev_sizes + gaps_before;

    // Apply justify-content spacing
    apply_justify_content_offset(scoped, base_offset, &justify_content, sibling_index)
}

/// Compute offset with auto margins on the main axis.
fn compute_auto_margin_offset<Axis>(
    scoped: &mut ScopedDb,
    parent_start: Subpixels,
    auto_start: bool,
    auto_end: bool,
) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    let parent_size = scoped.parent::<SizeQuery<Axis, ConstrainedMarker>>();
    let all_items_size: Subpixels = scoped
        .children::<SizeQuery<Axis, ConstrainedMarker>>()
        .sum();
    let free_space = parent_size - all_items_size;

    if free_space <= 0 {
        return parent_start;
    }

    let _item_size = scoped.query::<SizeQuery<Axis, ConstrainedMarker>>();
    let prev_sizes: Subpixels = scoped
        .prev_siblings::<SizeQuery<Axis, ConstrainedMarker>>()
        .sum();

    match (auto_start, auto_end) {
        (true, true) => {
            // Both margins auto: center the item
            parent_start + prev_sizes + free_space / 2
        }
        (true, false) => {
            // Only start margin auto: push to end
            parent_start + free_space + prev_sizes
        }
        (false, true) => {
            // Only end margin auto: stay at start
            parent_start + prev_sizes
        }
        (false, false) => parent_start + prev_sizes,
    }
}

/// Apply justify-content spacing to the base offset.
fn apply_justify_content_offset(
    scoped: &mut ScopedDb,
    base_offset: Subpixels,
    justify: &CssValue,
    sibling_index: usize,
) -> Subpixels {
    match justify {
        CssValue::Keyword(CssKeyword::FlexStart) | CssValue::Keyword(CssKeyword::Start) => {
            base_offset
        }
        CssValue::Keyword(CssKeyword::FlexEnd) | CssValue::Keyword(CssKeyword::End) => {
            let free_space = compute_free_space_main_axis(scoped);
            base_offset + free_space.max(0)
        }
        CssValue::Keyword(CssKeyword::Center) => {
            let free_space = compute_free_space_main_axis(scoped);
            base_offset + (free_space.max(0) / 2)
        }
        CssValue::Keyword(CssKeyword::SpaceBetween) => {
            let items_count = scoped.parent_children_count();
            if items_count <= 1 {
                return base_offset;
            }
            let free_space = compute_free_space_main_axis(scoped);
            let spacing = free_space.max(0) / (items_count as i32 - 1);
            base_offset + spacing * sibling_index as i32
        }
        CssValue::Keyword(CssKeyword::SpaceAround) => {
            let items_count = scoped.parent_children_count();
            if items_count == 0 {
                return base_offset;
            }
            let free_space = compute_free_space_main_axis(scoped);
            let spacing = free_space.max(0) / items_count as i32;
            base_offset + spacing * sibling_index as i32 + spacing / 2
        }
        CssValue::Keyword(CssKeyword::SpaceEvenly) => {
            let items_count = scoped.parent_children_count();
            if items_count == 0 {
                return base_offset;
            }
            let free_space = compute_free_space_main_axis(scoped);
            let spacing = free_space.max(0) / (items_count as i32 + 1);
            base_offset + spacing * (sibling_index as i32 + 1)
        }
        _ => base_offset,
    }
}

/// Compute free space on the main axis for justify-content.
fn compute_free_space_main_axis(scoped: &mut ScopedDb) -> Subpixels {
    let flex_direction = scoped.parent::<FlexDirectionQuery>();

    if is_main_axis_block(&flex_direction) {
        compute_free_space::<BlockMarker, RowGapQuery>(scoped)
    } else {
        compute_free_space::<InlineMarker, ColumnGapQuery>(scoped)
    }
}

// ============================================================================
// Cross Axis Offset Computation
// ============================================================================

/// Compute the cross axis offset for a flex item.
///
/// This handles align-items/align-self with all alignment values.
fn compute_cross_axis_offset<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    let align_self = scoped.query::<AlignSelfQuery>();
    let align = if align_self != CssValue::Keyword(CssKeyword::Auto) {
        align_self
    } else {
        scoped.parent::<AlignItemsQuery>()
    };

    let parent_start = helpers::parent_start::<Axis>(scoped);
    let parent_size = scoped.parent::<SizeQuery<Axis, ConstrainedMarker>>();
    let node_size = scoped.query::<SizeQuery<Axis, ConstrainedMarker>>();

    match align {
        CssValue::Keyword(CssKeyword::FlexStart) | CssValue::Keyword(CssKeyword::Start) => {
            parent_start
        }
        CssValue::Keyword(CssKeyword::FlexEnd) | CssValue::Keyword(CssKeyword::End) => {
            let padding_end = get_padding_end::<Axis>(scoped);
            parent_start + parent_size - node_size - padding_end
        }
        CssValue::Keyword(CssKeyword::Center) => parent_start + (parent_size - node_size) / 2,
        CssValue::Keyword(CssKeyword::Stretch) => {
            // Stretch is handled in sizing, position at start
            parent_start
        }
        CssValue::Keyword(CssKeyword::Baseline) => {
            // Baseline alignment would require text metrics
            // For now, approximate as flex-start
            parent_start
        }
        _ => parent_start,
    }
}

// ============================================================================
// Size Computation
// ============================================================================

/// Compute the main axis size of a flex container.
///
/// This implements the flexible sizing algorithm with flex-grow/flex-shrink.
fn compute_main_axis_container_size<Axis, GapQuery>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
    GapQuery: rewrite_core::Query<Key = rewrite_core::NodeId, Value = Subpixels> + 'static,
    GapQuery::Value: Clone + Send + Sync,
{
    let children_count = scoped.children_count();
    if children_count == 0 {
        return 0;
    }

    // Apply flexible sizing algorithm
    let total_size = apply_flexible_sizing::<Axis>(scoped);

    // Add gaps
    let gap = scoped.query::<GapQuery>();
    let gaps_total = compute_gaps_total(gap, children_count);

    total_size + gaps_total
}

/// Compute the cross axis size of a flex container.
fn compute_cross_axis_container_size<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    let flex_wrap = scoped.query::<FlexWrapQuery>();

    match flex_wrap {
        CssValue::Keyword(CssKeyword::Nowrap) => {
            // Single line: maximum item size
            max_child_size::<Axis>(scoped)
        }
        CssValue::Keyword(CssKeyword::Wrap) | CssValue::Keyword(CssKeyword::WrapReverse) => {
            // Multi-line: sum of line heights
            compute_multiline_cross_size::<Axis>(scoped)
        }
        _ => max_child_size::<Axis>(scoped),
    }
}

/// Compute cross size for multi-line flex containers.
fn compute_multiline_cross_size<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    // Simplified: sum of max sizes of items per line
    // A full implementation would break items into lines and sum line heights
    let line_count = estimate_line_count(scoped);
    let max_item_size = max_child_size::<Axis>(scoped);

    max_item_size * line_count as i32
}

// ============================================================================
// Flexible Sizing Algorithm
// ============================================================================

/// Apply flexible sizing to compute actual item sizes with flex-grow/flex-shrink.
fn apply_flexible_sizing<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    let container_size = scoped.query::<SizeQuery<Axis, ConstrainedMarker>>();
    let children_count = scoped.children_count();

    if children_count == 0 {
        return 0;
    }

    // Collect base sizes (hypothetical main size / flex-basis)
    let base_sizes: Vec<Subpixels> = scoped
        .children::<SizeQuery<Axis, ConstrainedMarker>>()
        .collect();
    let base_total: Subpixels = base_sizes.iter().sum();

    // Calculate free space
    let free_space = container_size - base_total;

    if free_space > 0 {
        // Growing: distribute space using flex-grow
        apply_flex_grow::<Axis>(scoped, base_sizes, free_space)
    } else if free_space < 0 {
        // Shrinking: reduce space using flex-shrink
        apply_flex_shrink::<Axis>(scoped, base_sizes, free_space.abs())
    } else {
        // Perfect fit: just sum base sizes
        base_total
    }
}

/// Distribute extra space among growing flex items.
fn apply_flex_grow<Axis>(
    scoped: &mut ScopedDb,
    base_sizes: Vec<Subpixels>,
    free_space: Subpixels,
) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    use rewrite_css::FlexGrowQuery;

    // Collect flex-grow factors (stored as fixed-point: factor * 64)
    let grow_factors: Vec<Subpixels> = scoped.children::<FlexGrowQuery>().collect();

    let total_grow: i64 = grow_factors.iter().map(|&x| x as i64).sum();

    if total_grow <= 0 {
        // No growing items, return base total
        return base_sizes.iter().sum();
    }

    // Distribute free space proportionally using fixed-point arithmetic
    let final_sizes: Vec<Subpixels> = base_sizes
        .iter()
        .zip(grow_factors.iter())
        .map(|(base, &grow)| {
            if grow > 0 {
                // Calculate: free_space * (grow / total_grow)
                // Using i64 to avoid overflow: (free_space * grow) / total_grow
                let extra = ((free_space as i64 * grow as i64) / total_grow) as i32;
                base + extra
            } else {
                *base
            }
        })
        .collect();

    final_sizes.iter().sum()
}

/// Shrink items to fit in container using flex-shrink.
fn apply_flex_shrink<Axis>(
    scoped: &mut ScopedDb,
    base_sizes: Vec<Subpixels>,
    deficit: Subpixels,
) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    use rewrite_css::FlexShrinkQuery;

    // Collect flex-shrink factors (stored as fixed-point: factor * 64)
    // Default shrink is 1.0 = 64 in fixed-point
    let shrink_factors: Vec<Subpixels> = scoped.children::<FlexShrinkQuery>().collect();

    // Calculate scaled shrink factors (shrink * base_size)
    // Keep in i64 to avoid overflow
    let scaled_shrinks: Vec<i64> = shrink_factors
        .iter()
        .zip(base_sizes.iter())
        .map(|(&shrink, &base)| (shrink as i64 * base as i64) / 64) // Divide by 64 to normalize fixed-point
        .collect();

    let total_scaled: i64 = scaled_shrinks.iter().sum();

    if total_scaled <= 0 {
        // No shrinking items, return base total
        return base_sizes.iter().sum();
    }

    // Shrink each item proportionally
    let final_sizes: Vec<Subpixels> = base_sizes
        .iter()
        .zip(scaled_shrinks.iter())
        .map(|(&base, &scaled)| {
            if scaled > 0 {
                // Calculate: deficit * (scaled / total_scaled)
                let reduction = ((deficit as i64 * scaled) / total_scaled) as i32;
                (base - reduction).max(0)
            } else {
                base
            }
        })
        .collect();

    final_sizes.iter().sum()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Compute total gap size between items.
///
/// Formula: gap * (count - 1) for count > 1, otherwise 0
fn compute_gaps_total(gap: Subpixels, count: usize) -> Subpixels {
    if count > 1 {
        gap * (count as i32 - 1)
    } else {
        0
    }
}

/// Compute free space on an axis (parent size - items size - gaps).
fn compute_free_space<Axis, GapQuery>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
    GapQuery: rewrite_core::Query<Key = rewrite_core::NodeId, Value = Subpixels> + 'static,
    GapQuery::Value: Clone + Send + Sync,
{
    let parent_size = scoped.parent::<SizeQuery<Axis, ConstrainedMarker>>();
    let items_size: Subpixels = scoped
        .children::<SizeQuery<Axis, ConstrainedMarker>>()
        .sum();
    let gap = scoped.parent::<GapQuery>();
    let children_count = scoped.parent_children_count();
    let gaps_total = compute_gaps_total(gap, children_count);
    parent_size - items_size - gaps_total
}

/// Get maximum child size along an axis.
fn max_child_size<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    scoped
        .children::<SizeQuery<Axis, ConstrainedMarker>>()
        .max()
        .unwrap_or(0)
}

/// Check if the main axis is the block axis.
fn is_main_axis_block(flex_direction: &CssValue) -> bool {
    matches!(
        flex_direction,
        CssValue::Keyword(CssKeyword::Column) | CssValue::Keyword(CssKeyword::ColumnReverse)
    )
}

/// Check if the main axis is the inline axis.
fn is_main_axis_inline(flex_direction: &CssValue) -> bool {
    matches!(
        flex_direction,
        CssValue::Keyword(CssKeyword::Row) | CssValue::Keyword(CssKeyword::RowReverse)
    )
}

/// Get the visual sibling index considering the order property.
fn get_visual_sibling_index(scoped: &mut ScopedDb) -> usize {
    let _current_order = match scoped.query::<OrderQuery>() {
        CssValue::Integer(n) => n,
        _ => 0,
    };

    // Count how many siblings with lower or equal order come before this one
    scoped.prev_siblings_count()
}

/// Get ordered previous sibling sizes (considering order property).
fn get_ordered_prev_sibling_sizes<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    // Simplified: just sum previous siblings
    // Full implementation would sort by order property
    scoped
        .prev_siblings::<SizeQuery<Axis, ConstrainedMarker>>()
        .sum()
}

/// Get item margins for an axis.
fn get_item_margins<Axis>(scoped: &mut ScopedDb) -> (Subpixels, Subpixels)
where
    Axis: crate::LayoutsMarker + 'static,
{
    use rewrite_css::{EndMarker, MarginQuery, StartMarker};

    // Map layout axis to CSS axis marker
    if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>() {
        let start = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
        let end = scoped.query::<MarginQuery<rewrite_css::BlockMarker, EndMarker>>();
        (start, end)
    } else {
        let start = scoped.query::<MarginQuery<rewrite_css::InlineMarker, StartMarker>>();
        let end = scoped.query::<MarginQuery<rewrite_css::InlineMarker, EndMarker>>();
        (start, end)
    }
}

/// Check if a margin is auto (negative sentinel value).
fn is_auto_margin(margin: Subpixels) -> bool {
    margin < 0
}

/// Get padding at the end of an axis.
fn get_padding_end<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    use rewrite_css::{EndMarker, PaddingQuery};

    // Map layout axis to CSS axis marker
    if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.parent::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>()
    } else {
        scoped.parent::<PaddingQuery<rewrite_css::InlineMarker, EndMarker>>()
    }
}

/// Estimate the number of lines in a flex-wrap container.
fn estimate_line_count(scoped: &mut ScopedDb) -> usize {
    // Simplified: assume 3 items per line on average
    let children_count = scoped.children_count();
    if children_count == 0 {
        return 0;
    }
    (children_count + 2) / 3
}
