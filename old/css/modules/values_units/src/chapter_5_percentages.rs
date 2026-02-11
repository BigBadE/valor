//! CSS Values & Units Level 3 — §5 Percentages
//! Spec: <https://www.w3.org/TR/css-values-3/#percentages>

use crate::ParseError;
use cssparser::{Parser, Token};

/// A CSS <percentage>
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Percentage(pub f32); // stored as 0.0..=1.0

/// Parse a CSS <percentage> (§5.1).
///
/// # Errors
/// Returns `ParseError::UnexpectedToken` when the next token is not a `<percentage>`.
pub fn parse_percentage(input: &mut Parser) -> Result<Percentage, ParseError> {
    if let Ok(token) = input.next_including_whitespace_and_comments()
        && let Token::Percentage { unit_value, .. } = token.clone()
    {
        return Ok(Percentage(unit_value));
    }
    Err(ParseError::UnexpectedToken)
}
