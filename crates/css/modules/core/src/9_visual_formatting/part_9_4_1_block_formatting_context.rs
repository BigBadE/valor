//! Spec: CSS 2.2 §9.4.1 Block formatting context
//! Re-export BFC predicates and related helpers.

use css_orchestrator::style_model::{ComputedStyle, Display, Float, Overflow, Position};

/// Spec: §9.4.1 — Whether a given style establishes a new BFC.
pub const fn establishes_block_formatting_context(style: &ComputedStyle) -> bool {
    // Common BFC triggers:
    // - Float is not none
    // - Position is absolute/fixed
    // - Overflow establishes a BFC (anything other than visible)
    // - Flex containers establish a formatting context for their contents
    // Note: Clear does not itself establish a BFC; included for clarity in match list, unused.
    matches!(style.float, Float::Left | Float::Right)
        || matches!(style.position, Position::Absolute | Position::Fixed)
        || !matches!(style.overflow, Overflow::Visible)
        || matches!(style.display, Display::Flex | Display::InlineFlex)
}
