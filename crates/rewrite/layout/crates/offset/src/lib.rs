//! Offset query - computes position offset along an axis.

use rewrite_core::ScopedDb;
use rewrite_css::{CssKeyword, CssValue, DisplayQuery, Subpixels};
use rewrite_layout_util::{Axis, Dispatcher, LayoutType};

/// Offset mode enumeration - re-exported from offset_impl.
pub use rewrite_layout_offset_impl::OffsetMode;

// Import SizeDispatcher from size - offset depends on size
use rewrite_layout_size::SizeDispatcher as SizeDispatcherType;

/// Offset property query - returns position offset along an axis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(Subpixels)]
pub enum OffsetProperty {
    #[query(compute_offset)]
    #[params(
        rewrite_layout_util::AxisMarker,
        rewrite_layout_offset_impl::OffsetModeMarker
    )]
    Offset,
}

// The macro automatically generates type aliases like OffsetQuery<Axis, Mode>

/// Offset dispatcher - used by layout modules to recursively query offsets.
pub struct OffsetDispatcher;

impl Dispatcher<(Axis, OffsetMode)> for OffsetDispatcher {
    type Returns = Subpixels;

    fn query(scoped: &mut ScopedDb, param: (Axis, OffsetMode)) -> Self::Returns {
        let (axis, mode) = param;

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
                // For flex, we need to pass both OffsetDispatcher and the concrete SizeDispatcher from layout crate
                // The SizeDispatcher is obtained from the layout crate which knows about FlexImpl
                rewrite_layout_flex::compute_flex_offset::<
                    OffsetDispatcher,
                    SizeDispatcherType<rewrite_layout_flex::FlexSize>,
                >(scoped, axis, mode)
            }
            LayoutType::Grid => {
                rewrite_layout_grid::compute_grid_offset::<OffsetDispatcher>(scoped, axis, mode)
            }
            LayoutType::Block => rewrite_layout_block::compute_block_offset::<
                OffsetDispatcher,
                SizeDispatcherType<rewrite_layout_flex::FlexSize>,
            >(scoped, axis, mode),
            LayoutType::Inline => rewrite_layout_block::compute_block_offset::<
                OffsetDispatcher,
                SizeDispatcherType<rewrite_layout_flex::FlexSize>,
            >(scoped, axis, mode),
            LayoutType::Float => {
                rewrite_layout_float::compute_float_offset::<OffsetDispatcher>(scoped, axis, mode)
            }
        }
    }
}

// ============================================================================
// Query Implementation
// ============================================================================

fn compute_offset(scoped: &mut ScopedDb, axis: Axis, mode: OffsetMode) -> Subpixels {
    OffsetDispatcher::query(scoped, (axis, mode))
}
