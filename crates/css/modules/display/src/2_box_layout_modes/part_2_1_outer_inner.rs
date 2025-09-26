//! Spec: CSS Display 3 — §2.1 Outer roles and §2.2 Inner models
//! <https://www.w3.org/TR/css-display-3/#outer-role>
//! <https://www.w3.org/TR/css-display-3/#inner-model>

use css_orchestrator::style_model::Display;

#[inline]
/// Return true if the element participates as a block-level box in flow layout.
///
/// Notes:
/// - Our public `Display` does not currently model run-in, table, grid, or ruby.
/// - Flex and `InlineFlex` are treated as block-level containers for the purpose of
///   anonymous block delimitation and block formatting.
pub const fn is_block_level_outer(display: Display) -> bool {
    matches!(
        display,
        Display::Block | Display::Flex | Display::InlineFlex
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(
        clippy::missing_panics_doc,
        reason = "simple assertions in tiny unit test"
    )]
    fn block_level_helper_matches_expectations() {
        assert!(is_block_level_outer(Display::Block));
        assert!(is_block_level_outer(Display::Flex));
        assert!(is_block_level_outer(Display::InlineFlex));
        assert!(!is_block_level_outer(Display::Inline));
        assert!(!is_block_level_outer(Display::None));
        assert!(!is_block_level_outer(Display::Contents));
    }
}
