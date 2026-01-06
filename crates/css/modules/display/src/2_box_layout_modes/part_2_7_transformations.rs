//! Spec: CSS Display 3 — §2.7 Automatic Box Type Transformations
//! <https://www.w3.org/TR/css-display-3/#transformations>

use css_orchestrator::style_model::{ComputedStyle, Display, Float, Position};

/// Blockify a display value according to CSS Display 3 §2.7.
///
/// Blockification converts the outer display type to block while preserving the inner display type:
/// - inline → block
/// - inline-block → block
/// - inline-flex → flex
/// - inline-grid → grid
/// - Other values remain unchanged (they already have block outer display)
const fn blockify(display: Display) -> Display {
    match display {
        Display::Inline | Display::InlineBlock => Display::Block,
        Display::InlineFlex => Display::Flex,
        Display::InlineGrid => Display::Grid,
        // Already block-level or special values
        Display::Block | Display::Flex | Display::Grid | Display::None | Display::Contents => {
            display
        }
    }
}

/// Compute the used outer display value for a child box, applying blockification/inlinification
/// rules relevant to our current layout engine. This is a pure function that does not mutate styles.
///
/// Spec mapping:
/// - Blockify when: absolute positioning or floating (CSS2), parent is flex/grid container (by
///   mapping in style engine we currently only expose Flex/InlineFlex, which produces flex items).
/// - Preserve `display: none` and `display: contents` early (no box or pass-through), callers should
///   already have applied §2.5 normalization for contents/none before layout.
/// - Root element handling is covered by §2.8. Callers can pass `is_root = true` to enforce root rules.
///
/// Notes:
/// - Our public `Display` enum currently exposes Block, Inline, None, Flex, `InlineFlex`, Contents.
///   Grid/Ruby/Table layout are handled in other modules; for now, flex container blockification is
///   sufficient for our fixtures and keeps behavior aligned with Chromium comparisons.
pub const fn used_display_for_child(
    child: &ComputedStyle,
    parent: Option<&ComputedStyle>,
    is_root: bool,
) -> Display {
    // §2.5: values that suppress or elide boxes are preserved for upstream handling.
    match child.display {
        Display::None | Display::Contents => return child.display,
        _ => {}
    }

    // §2.8: Root element is always blockified and establishes an independent formatting context.
    if is_root {
        return blockify(child.display);
    }

    // CSS2: Absolute positioning or floating blockifies the outer display type.
    if !matches!(child.position, Position::Static | Position::Relative) {
        return blockify(child.display);
    }
    if !matches!(child.float, Float::None) {
        return blockify(child.display);
    }

    // Flex/Grid containers make their children flex/grid items. Our engine exposes Flex/InlineFlex.
    if let Some(parent_style) = parent
        && matches!(
            parent_style.display,
            Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid
        )
    {
        return blockify(child.display);
    }

    // Default: keep specified display.
    child.display
}
