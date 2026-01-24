//! Size query - computes size along an axis with a given mode.

use rewrite_core::ScopedDb;
use rewrite_css::{CssKeyword, CssValue, DisplayQuery, Subpixels};
use rewrite_layout_size_impl::{FlexSizeDispatcher, SizeDispatcher as SizeDispatcherTrait};
use rewrite_layout_util::{Axis, Dispatcher, LayoutType};

/// Size mode enumeration - re-exported from size_impl.
pub use rewrite_layout_size_impl::SizeMode;

/// Dispatcher for recursive size queries.
/// Generic over FlexImpl to avoid circular dependencies.
pub struct SizeDispatcher<FlexImpl>(std::marker::PhantomData<FlexImpl>);

impl<FlexImpl> SizeDispatcherTrait for SizeDispatcher<FlexImpl>
where
    FlexImpl: FlexSizeDispatcher,
{
    fn query(scoped: &mut ScopedDb, axis: Axis, mode: SizeMode) -> Subpixels {
        compute_size::<FlexImpl>(scoped, axis, mode)
    }
}

impl<FlexImpl> Dispatcher<(Axis, SizeMode)> for SizeDispatcher<FlexImpl>
where
    FlexImpl: FlexSizeDispatcher,
{
    type Returns = Subpixels;

    fn query(scoped: &mut ScopedDb, param: (Axis, SizeMode)) -> Self::Returns {
        let (axis, mode) = param;
        compute_size::<FlexImpl>(scoped, axis, mode)
    }
}

/// Re-export the concrete SizeQuery instantiated with a FlexImpl.
/// This type is generic over FlexImpl to avoid circular dependencies.
/// Use `make_size_query::<YourFlexImpl>()` to get the concrete query type.
pub type SizeQueryGeneric<AxisParam, ModeParam, FlexImpl> =
    rewrite_layout_size_impl::DispatchedSizeQuery<
        AxisParam,
        ModeParam,
        SizeDispatcher<FlexImpl>,
        FlexImpl,
    >;

// ============================================================================
// Query Implementation
// ============================================================================

fn compute_size<FlexImpl>(scoped: &mut ScopedDb, axis: Axis, mode: SizeMode) -> Subpixels
where
    FlexImpl: FlexSizeDispatcher,
{
    // Get the display property to determine which layout algorithm to use
    let display = scoped.query::<DisplayQuery>();

    // Return 0 for display:none elements
    if matches!(display, CssValue::Keyword(CssKeyword::None)) {
        return 0;
    }

    // TODO: Also check if element should be display:none by default (UA stylesheet)
    // For now, hardcode common elements that should be hidden
    use rewrite_html::TagNameQuery;
    if let Some(tag_name) = scoped.query::<TagNameQuery>() {
        let tag_str: &str = tag_name.as_str();
        if matches!(
            tag_str,
            "head" | "script" | "style" | "meta" | "link" | "title"
        ) {
            return 0;
        }
    }

    // Determine the layout type from display value
    let layout_type = match display {
        CssValue::Keyword(CssKeyword::Flex) | CssValue::Keyword(CssKeyword::InlineFlex) => {
            LayoutType::Flex
        }
        CssValue::Keyword(CssKeyword::Grid) | CssValue::Keyword(CssKeyword::InlineGrid) => {
            LayoutType::Grid
        }
        CssValue::Keyword(CssKeyword::Block) => LayoutType::Block,
        CssValue::Keyword(CssKeyword::Inline) => LayoutType::Inline,
        _ => LayoutType::Block,
    };

    // Call the appropriate layout module
    match layout_type {
        LayoutType::Flex => {
            FlexImpl::compute_flex_size::<SizeDispatcher<FlexImpl>>(scoped, axis, mode)
        }
        LayoutType::Grid => {
            rewrite_layout_grid::compute_grid_size::<SizeDispatcher<FlexImpl>>(scoped, axis, mode)
        }
        LayoutType::Block => {
            rewrite_layout_block::compute_block_size::<SizeDispatcher<FlexImpl>>(scoped, axis, mode)
        }
        LayoutType::Inline => {
            rewrite_layout_block::compute_block_size::<SizeDispatcher<FlexImpl>>(scoped, axis, mode)
        }
        LayoutType::Float => {
            rewrite_layout_float::compute_float_size::<SizeDispatcher<FlexImpl>>(scoped, axis, mode)
        }
    }
}
