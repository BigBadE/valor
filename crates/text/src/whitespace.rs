//! CSS Text 3 §4.1.1 whitespace collapsing for `white-space: normal`.
//!
//! Implements Phase I (collapsing and transformation) and a Phase II
//! approximation (trimming at block boundaries).

use std::iter::Peekable;
use std::str::CharIndices;

type CharIter<'src> = Peekable<CharIndices<'src>>;

/// Consume spaces and tabs from the iterator.
fn eat_spaces_tabs(chars: &mut CharIter<'_>) {
    while let Some(&(_, next)) = chars.peek() {
        if next == ' ' || next == '\t' {
            chars.next();
        } else {
            break;
        }
    }
}

/// After encountering a segment break, consume any trailing spaces/tabs
/// and additional consecutive segment breaks (Step 1 + Step 2).
fn consume_segment_break_region(chars: &mut CharIter<'_>) {
    loop {
        eat_spaces_tabs(chars);
        match chars.peek() {
            Some(&(_, '\n')) => {
                chars.next();
            }
            Some(&(_, '\r')) => {
                chars.next();
                if let Some(&(_, '\n')) = chars.peek() {
                    chars.next();
                }
            }
            _ => break,
        }
    }
}

/// Collapse whitespace per CSS Text 3 §4.1.1 (`white-space: normal`).
///
/// Phase I steps:
/// 1. Remove spaces/tabs immediately adjacent to segment breaks (newlines).
/// 2. Convert remaining segment breaks to spaces.
/// 3. Convert tabs to spaces.
/// 4. Collapse consecutive spaces to a single space.
///
/// Phase II approximation:
/// - Strip leading space if `at_block_start` (first content in block).
/// - Strip trailing space if `at_block_end` (last content in block).
///
/// Inter-element spaces (e.g. between a text node and a sibling `<span>`)
/// are preserved as single spaces when not at block boundaries.
pub fn collapse_whitespace(text: &str, at_block_start: bool, at_block_end: bool) -> String {
    // Phase I, Steps 1–2: process segment breaks.
    //
    // For each segment break (newline):
    //   - Remove all spaces/tabs immediately before it.
    //   - Remove all spaces/tabs immediately after it.
    //   - Replace the segment break itself with a single space.
    //
    // We do this by scanning character-by-character, tracking whether
    // we are in a "segment break region" (newline plus adjacent spaces/tabs).
    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some((_, cur)) = chars.next() {
        if cur == '\n' || cur == '\r' {
            // Skip \r\n as a single segment break.
            if cur == '\r'
                && let Some(&(_, '\n')) = chars.peek()
            {
                chars.next();
            }
            consume_segment_break_region(&mut chars);
            // Step 2: replace the entire segment break region with one space.
            result.push(' ');
        } else if cur == ' ' || cur == '\t' {
            // Consume all consecutive spaces/tabs and check what follows.
            let space_start = result.len();
            result.push(' '); // Step 3: tabs become spaces; Step 4: collapse.
            eat_spaces_tabs(&mut chars);

            // If a segment break follows, remove the space we just pushed
            // (Step 1: spaces before a segment break are removed).
            if let Some(&(_, next)) = chars.peek()
                && (next == '\n' || next == '\r')
            {
                result.truncate(space_start);
            }
        } else {
            result.push(cur);
        }
    }

    // Phase II approximation: trim at block boundaries.
    if at_block_start && result.starts_with(' ') {
        result.remove(0);
    }
    if at_block_end && result.ends_with(' ') {
        result.pop();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_collapse() {
        assert_eq!(
            collapse_whitespace("hello  world", false, false),
            "hello world"
        );
    }

    #[test]
    fn newline_with_surrounding_spaces() {
        assert_eq!(
            collapse_whitespace(
                "\n      Color should inherit from body\n      ",
                false,
                false
            ),
            " Color should inherit from body "
        );
    }

    #[test]
    fn at_block_start_trims_leading() {
        assert_eq!(
            collapse_whitespace(
                "\n      Color should inherit from body\n      ",
                true,
                false
            ),
            "Color should inherit from body "
        );
    }

    #[test]
    fn at_block_end_trims_trailing() {
        assert_eq!(
            collapse_whitespace(
                "\n      Color should inherit from body\n      ",
                false,
                true
            ),
            " Color should inherit from body"
        );
    }

    #[test]
    fn at_both_boundaries() {
        assert_eq!(
            collapse_whitespace("\n      Color should inherit from body\n      ", true, true),
            "Color should inherit from body"
        );
    }

    #[test]
    fn tabs_converted() {
        assert_eq!(collapse_whitespace("a\tb", false, false), "a b");
    }

    #[test]
    fn tabs_before_newline_removed() {
        assert_eq!(collapse_whitespace("a\t\nb", false, false), "a b");
    }

    #[test]
    fn multiple_newlines() {
        // Step 2: consecutive segment breaks collapse to one space.
        assert_eq!(collapse_whitespace("a\n\nb", false, false), "a b");
    }

    #[test]
    fn only_whitespace() {
        assert_eq!(collapse_whitespace("   \n   ", true, true), "");
    }

    #[test]
    fn no_whitespace() {
        assert_eq!(collapse_whitespace("hello", false, false), "hello");
    }

    #[test]
    fn crlf_handling() {
        assert_eq!(collapse_whitespace("a\r\nb", false, false), "a b");
    }
}
