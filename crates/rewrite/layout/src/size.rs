use crate::{
    BlockMarker, BlockSizeQuery, InlineMarker, InlineSizeQuery, IntrinsicBlockSizeQuery,
    IntrinsicInlineSizeQuery, Layouts, SizeMode, Subpixels,
    formatting_contexts::{flex, grid},
    helpers,
};
use rewrite_core::{Database, DependencyContext, NodeId, ScopedDb};
use rewrite_css::{
    BorderWidthQuery, CssKeyword, CssValue, DisplayQuery, EndMarker, MarginQuery, PaddingQuery,
    StartMarker,
};

pub fn compute_size(
    db: &Database,
    node: NodeId,
    axis: Layouts,
    mode: SizeMode,
    ctx: &mut DependencyContext,
) -> Subpixels {
    let mut scoped = ScopedDb::new(db, node, ctx);

    // First check for explicit width/height
    if let Some(explicit_size) = get_explicit_size(db, node, scoped.ctx_mut(), axis) {
        return add_padding_and_border(&mut scoped, explicit_size, axis);
    }

    match axis {
        Layouts::Block => {
            let display = scoped.query::<DisplayQuery>();

            match (&display, mode) {
                (CssValue::Keyword(CssKeyword::Block), SizeMode::Constrained)
                | (CssValue::Keyword(CssKeyword::InlineBlock), SizeMode::Constrained) => {
                    let content_size = scoped.children::<BlockSizeQuery>().sum();
                    add_padding_and_border(&mut scoped, content_size, axis)
                }
                (CssValue::Keyword(CssKeyword::Flex), SizeMode::Constrained) => {
                    flex::compute_flex_size(&mut scoped, axis, mode)
                }
                (CssValue::Keyword(CssKeyword::Grid), _) => {
                    grid::compute_grid_size(&mut scoped, axis, mode)
                }
                _ => {
                    // Intrinsic mode or other display modes
                    if mode == SizeMode::Intrinsic {
                        // Base case: intrinsic block size is sum of children's intrinsic sizes
                        let content_size = scoped.children::<IntrinsicBlockSizeQuery>().sum();
                        add_padding_and_border(&mut scoped, content_size, axis)
                    } else {
                        // Fallback to intrinsic for unknown display modes
                        scoped.query::<IntrinsicBlockSizeQuery>()
                    }
                }
            }
        }
        Layouts::Inline => {
            let display = scoped.query::<DisplayQuery>();

            match (&display, mode) {
                (CssValue::Keyword(CssKeyword::Flex), SizeMode::Constrained) => {
                    flex::compute_flex_size(&mut scoped, axis, mode)
                }
                (CssValue::Keyword(CssKeyword::Grid), SizeMode::Constrained) => {
                    grid::compute_grid_size(&mut scoped, axis, mode)
                }
                (CssValue::Keyword(CssKeyword::Block), SizeMode::Constrained) => {
                    use rewrite_core::Relationship;

                    // Check if we have a parent
                    let parent_ids = scoped
                        .db()
                        .resolve_relationship(scoped.node(), Relationship::Parent);
                    if parent_ids.is_empty() {
                        // No parent - use viewport width
                        use rewrite_css::{ViewportInput, ViewportSize};
                        let viewport = scoped
                            .db()
                            .get_input::<ViewportInput>(&())
                            .unwrap_or_else(ViewportSize::default);
                        return (viewport.width * 64.0) as Subpixels;
                    }

                    let parent_inline_size = scoped.parent::<InlineSizeQuery>();
                    let parent_padding = helpers::parent_padding_sum_inline(&mut scoped);
                    let parent_border = {
                        let start = scoped
                            .parent::<BorderWidthQuery<rewrite_css::InlineMarker, StartMarker>>();
                        let end = scoped
                            .parent::<BorderWidthQuery<rewrite_css::InlineMarker, EndMarker>>();
                        start + end
                    };
                    // Subtract own margins, not parent's
                    let own_margin_inline = {
                        let start =
                            scoped.query::<MarginQuery<rewrite_css::InlineMarker, StartMarker>>();
                        let end =
                            scoped.query::<MarginQuery<rewrite_css::InlineMarker, EndMarker>>();
                        start + end
                    };
                    parent_inline_size - parent_padding - parent_border - own_margin_inline
                }

                _ => {
                    // Intrinsic mode or other display modes
                    if mode == SizeMode::Intrinsic {
                        // Base case: intrinsic inline size depends on display type
                        let content_size = match &display {
                            // For inline elements, sum children (they flow horizontally)
                            CssValue::Keyword(CssKeyword::Inline) => {
                                scoped.children::<IntrinsicInlineSizeQuery>().sum()
                            }
                            // For block and other elements, take max (they stack vertically)
                            _ => scoped
                                .children::<IntrinsicInlineSizeQuery>()
                                .max()
                                .unwrap_or(0),
                        };
                        add_padding_and_border(&mut scoped, content_size, axis)
                    } else {
                        // Fallback to intrinsic for unknown display modes
                        scoped.query::<IntrinsicInlineSizeQuery>()
                    }
                }
            }
        }
    }
}

/// Get explicit size from width/height CSS properties.
fn get_explicit_size(
    db: &Database,
    node: NodeId,
    ctx: &mut DependencyContext,
    axis: Layouts,
) -> Option<Subpixels> {
    use rewrite_css::storage::{InheritedCssPropertyQuery, css_value_to_subpixels};

    let property = match axis {
        Layouts::Block => "height",
        Layouts::Inline => "width",
    };

    // Query the raw CSS value
    let value = db.query::<InheritedCssPropertyQuery>((node, property.to_string()), ctx);

    // Check if it's not 'auto' or a keyword
    match &value {
        CssValue::Keyword(CssKeyword::Auto) | CssValue::Keyword(_) => None,
        _ => {
            // Use proper conversion with containing block context
            // For now, pass None for containing block size (will be handled properly later)
            let subpixels = css_value_to_subpixels(&value, node, db, None);
            if subpixels > 0 { Some(subpixels) } else { None }
        }
    }
}

/// Add padding and border to a content size.
fn add_padding_and_border(
    scoped: &mut ScopedDb,
    content_size: Subpixels,
    axis: Layouts,
) -> Subpixels {
    let (padding, border) = match axis {
        Layouts::Block => {
            let padding_start =
                scoped.query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>();
            let padding_end = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>();
            let border_start =
                scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>();
            let border_end =
                scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, EndMarker>>();
            (padding_start + padding_end, border_start + border_end)
        }
        Layouts::Inline => {
            let padding_start =
                scoped.query::<PaddingQuery<rewrite_css::InlineMarker, StartMarker>>();
            let padding_end = scoped.query::<PaddingQuery<rewrite_css::InlineMarker, EndMarker>>();
            let border_start =
                scoped.query::<BorderWidthQuery<rewrite_css::InlineMarker, StartMarker>>();
            let border_end =
                scoped.query::<BorderWidthQuery<rewrite_css::InlineMarker, EndMarker>>();
            (padding_start + padding_end, border_start + border_end)
        }
    };

    content_size + padding + border
}
