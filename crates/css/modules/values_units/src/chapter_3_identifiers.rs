//! CSS Identifiers (used widely across CSS values)
//! Spec: <https://www.w3.org/TR/CSS2/syndata.html#value-def-identifier>

use crate::ParseError;
use cssparser::{Parser, Token};

/// A CSS identifier value (lowercased for canonicalization in our MVP).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident(pub String);

/// Parse a CSS identifier token.
///
/// # Errors
/// Returns `ParseError::UnexpectedToken` when the next token is not an identifier.
#[inline]
pub fn parse_ident(input: &mut Parser) -> Result<Ident, ParseError> {
    input.next_including_whitespace_and_comments().map_or(
        Err(ParseError::UnexpectedToken),
        |token| match token.clone() {
            Token::Ident(text) => Ok(Ident(text.as_ref().to_ascii_lowercase())),
            Token::AtKeyword(_)
            | Token::Hash(_)
            | Token::IDHash(_)
            | Token::QuotedString(_)
            | Token::UnquotedUrl(_)
            | Token::Delim(_)
            | Token::Number { .. }
            | Token::Percentage { .. }
            | Token::Dimension { .. }
            | Token::WhiteSpace(_)
            | Token::Comment(_)
            | Token::Colon
            | Token::Semicolon
            | Token::Comma
            | Token::IncludeMatch
            | Token::DashMatch
            | Token::PrefixMatch
            | Token::SuffixMatch
            | Token::SubstringMatch
            | Token::CDO
            | Token::CDC
            | Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock
            | Token::BadUrl(_)
            | Token::BadString(_)
            | Token::CloseParenthesis
            | Token::CloseSquareBracket
            | Token::CloseCurlyBracket => Err(ParseError::UnexpectedToken),
        },
    )
}
