use crate::{
    BlockMarker, BlockOffsetQuery, ConstrainedMarker, InlineMarker, IntrinsicMarker, Layouts,
    OffsetQuery, SizeQuery, Subpixels,
    formatting_contexts::{flex, grid},
    helpers,
};
use rewrite_core::{Database, DependencyContext, NodeId, ScopedDb};
use rewrite_css::{CssKeyword, CssValue, DisplayQuery, PositionQuery};

pub fn compute_offset(
    db: &Database,
    node: NodeId,
    axis: Layouts,
    ctx: &mut DependencyContext,
) -> Subpixels {
    let mut scoped = ScopedDb::new(db, node, ctx);

    // Check if element is floated first (floats are removed from normal flow)
    let float_value = scoped.query::<rewrite_css::FloatQuery>();
    if !matches!(float_value, CssValue::Keyword(CssKeyword::None)) {
        let static_offset = compute_static_offset(&mut scoped, axis);
        return crate::formatting_contexts::float_layout::compute_float_offset(
            &mut scoped,
            axis,
            static_offset,
        );
    }

    let position = scoped.query::<PositionQuery>();

    match &position {
        CssValue::Keyword(CssKeyword::Absolute) | CssValue::Keyword(CssKeyword::Fixed) => {
            match axis {
                Layouts::Block => compute_positioned_offset::<BlockMarker>(&mut scoped, axis),
                Layouts::Inline => compute_positioned_offset::<InlineMarker>(&mut scoped, axis),
            }
        }
        CssValue::Keyword(CssKeyword::Relative) => match axis {
            Layouts::Block => compute_relative_offset::<BlockMarker>(&mut scoped),
            Layouts::Inline => compute_relative_offset::<InlineMarker>(&mut scoped),
        },
        _ => compute_static_offset(&mut scoped, axis),
    }
}

/// Compute absolute/fixed positioned offset for an axis.
fn compute_positioned_offset<Axis>(scoped: &mut ScopedDb, axis: Layouts) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    use rewrite_css::{EndMarker, PositionOffsetQuery, StartMarker};

    // Map layout axis to CSS axis marker and query start position
    let start = if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
    };

    if start > 0 {
        let parent_offset = scoped.parent::<OffsetQuery<Axis>>();
        return parent_offset + start;
    }

    // Try end position (bottom for block, right for inline)
    let end = if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, EndMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, EndMarker>>()
    };

    if end > 0 {
        let parent_offset = scoped.parent::<OffsetQuery<Axis>>();
        let parent_size = scoped.parent::<SizeQuery<Axis, ConstrainedMarker>>();
        // Use intrinsic size for positioned elements since they're removed from flow
        let node_size = scoped.query::<SizeQuery<Axis, IntrinsicMarker>>();
        return parent_offset + parent_size - end - node_size;
    }

    // Neither specified, use static position
    compute_static_offset(scoped, axis)
}

/// Compute relative positioned offset for an axis.
fn compute_relative_offset<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    use rewrite_css::{PositionOffsetQuery, StartMarker};

    let static_offset = scoped.query::<OffsetQuery<Axis>>();

    // Map layout axis to CSS axis marker
    let position_offset = if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>()
    {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
    };

    if position_offset > 0 {
        static_offset + position_offset
    } else {
        static_offset
    }
}

fn compute_static_offset(scoped: &mut ScopedDb, axis: Layouts) -> Subpixels {
    let parent_display = scoped.parent::<DisplayQuery>();

    match &parent_display {
        CssValue::Keyword(CssKeyword::Block) => match axis {
            Layouts::Block => compute_block_offset(scoped),
            Layouts::Inline => compute_inline_offset(scoped),
        },
        CssValue::Keyword(CssKeyword::Flex) => flex::compute_flex_offset(scoped, axis),
        CssValue::Keyword(CssKeyword::Grid) => grid::compute_grid_offset(scoped, axis),
        CssValue::Keyword(CssKeyword::Inline) => match axis {
            Layouts::Block => scoped.parent::<BlockOffsetQuery>(),
            Layouts::Inline => helpers::get_offset::<InlineMarker>(scoped),
        },
        _ => 0,
    }
}

/// Compute inline offset in block formatting context.
///
/// In block formatting context, elements are positioned horizontally starting from
/// the parent's content edge, plus the element's own left margin.
fn compute_inline_offset(scoped: &mut ScopedDb) -> Subpixels {
    use rewrite_css::{MarginQuery, StartMarker};

    let parent_start = helpers::parent_start::<InlineMarker>(scoped);
    let margin_left = scoped.query::<MarginQuery<rewrite_css::InlineMarker, StartMarker>>();

    parent_start + margin_left
}

/// Compute block offset with margin collapsing.
///
/// In normal block flow, elements are stacked vertically with margins between them.
/// Adjacent margins collapse according to CSS margin collapsing rules.
fn compute_block_offset(scoped: &mut ScopedDb) -> Subpixels {
    use crate::positioning::margin;
    use rewrite_css::{MarginQuery, StartMarker};

    let parent_start = helpers::parent_start::<BlockMarker>(scoped);

    // Sum previous siblings' sizes
    // Use intrinsic size to avoid circular dependency (offset needs size, size needs children's offsets)
    let prev_siblings: Vec<Subpixels> = scoped
        .prev_siblings::<SizeQuery<BlockMarker, IntrinsicMarker>>()
        .collect();
    let prev_sizes: Subpixels = prev_siblings.iter().sum();
    let has_prev_siblings = !prev_siblings.is_empty();

    eprintln!(
        "Node {:?}: prev_siblings.len()={}, prev_sizes={}, has_prev_siblings={}",
        scoped.node(),
        prev_siblings.len(),
        prev_sizes,
        has_prev_siblings
    );

    // Determine the margin to use for this element's position
    let margin = if !has_prev_siblings {
        // This is the first child in its parent
        // Check if we can collapse with our parent
        if margin::can_collapse_with_parent_start(scoped) {
            // Our margin collapses with parent - use margin=0
            // The parent's offset calculation will include the collapsed margin
            eprintln!(
                "Node {:?}: First child, collapsing with parent, margin=0",
                scoped.node()
            );
            0
        } else {
            // Can't collapse with parent (parent has padding/border)
            // Use our own collapsed margin (which may collapse with our first child)
            let m = margin::get_margin_for_offset(scoped);
            eprintln!(
                "Node {:?}: First child, NOT collapsing with parent, margin={}",
                scoped.node(),
                m
            );
            m
        }
    } else {
        // We have previous siblings - get the collapsed margin
        // (accounting for collapsing with previous sibling)
        let m = margin::get_effective_margin_start(scoped);
        eprintln!("Node {:?}: Has prev siblings, margin={}", scoped.node(), m);
        m
    };

    let result = parent_start + prev_sizes + margin;
    eprintln!(
        "Node {:?}: offset = parent_start({}) + prev_sizes({}) + margin({}) = {}",
        scoped.node(),
        parent_start,
        prev_sizes,
        margin,
        result
    );
    result
}
