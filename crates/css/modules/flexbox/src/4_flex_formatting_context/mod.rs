//! Flex Formatting Context (FFC)
//! Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-containers>

/// Display keywords relevant to flex detection.
///
/// Spec: <https://www.w3.org/TR/css-display-3/#the-display-properties>
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum DisplayKeyword {
    Inline,
    Block,
    None,
    Flex,
    InlineFlex,
}

/// Returns true when the element establishes a Flex Formatting Context (FFC).
///
/// Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-containers>
#[inline]
pub const fn establishes_flex_formatting_context(display: DisplayKeyword) -> bool {
    matches!(display, DisplayKeyword::Flex | DisplayKeyword::InlineFlex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// # Panics
    /// Panics if `establishes_flex_formatting_context` does not return true for Flex/InlineFlex.
    fn flex_formats_established_for_flex_keywords() {
        assert!(establishes_flex_formatting_context(DisplayKeyword::Flex));
        assert!(establishes_flex_formatting_context(
            DisplayKeyword::InlineFlex
        ));
    }

    #[test]
    /// # Panics
    /// Panics if `establishes_flex_formatting_context` returns true for non-flex display keywords.
    fn non_flex_keywords_do_not_establish() {
        assert!(!establishes_flex_formatting_context(DisplayKeyword::Inline));
        assert!(!establishes_flex_formatting_context(DisplayKeyword::Block));
        assert!(!establishes_flex_formatting_context(DisplayKeyword::None));
    }
}
