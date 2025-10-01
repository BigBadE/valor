//! CSS Values & Units Level 3 — §4 Numbers
//! Spec: <https://www.w3.org/TR/css-values-3/#numeric-types>

use crate::ParseError;
use cssparser::Parser;
use cssparser::Token;

/// A CSS <number>
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Number(pub f32);

/// Parse a CSS <number> (§4.2). Accepts integer or real numbers.
///
/// # Errors
/// Returns `ParseError::UnexpectedToken` when the next token is not a `<number>`.
pub fn parse_number(input: &mut Parser) -> Result<Number, ParseError> {
    input.next_including_whitespace_and_comments().map_or(
        Err(ParseError::UnexpectedToken),
        |token| {
            if let Token::Number { value, .. } = token.clone() {
                Ok(Number(value))
            } else {
                Err(ParseError::UnexpectedToken)
            }
        },
    )
}
