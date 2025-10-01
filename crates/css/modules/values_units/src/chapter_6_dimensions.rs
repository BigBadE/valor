//! CSS Values & Units Level 3 — §6 Dimensions (Lengths subset)
//! Spec: <https://www.w3.org/TR/css-values-3/#lengths>

use crate::ParseError;
use cssparser::{Parser, Token};

/// Supported subset of CSS <length>: px, em, rem, plus unitless zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthUnit {
    Pixels,
    Ems,
    RootEms,
    ViewportWidth,
    ViewportHeight,
}

/// Compute the pixel value for a given `Length` using the current environment.
///
/// - Pixels: returns the raw value.
/// - Ems/RootEms: scales by the provided font sizes.
/// - Viewport-relative (vw/vh): requires viewport; returns a percentage of width/height.
pub fn compute_length_px(
    length: Length,
    font_size_px: f32,
    root_font_size_px: f32,
    viewport: Option<Viewport>,
) -> Option<f32> {
    match length.unit {
        LengthUnit::Pixels => Some(length.value),
        LengthUnit::Ems => Some(length.value * font_size_px),
        LengthUnit::RootEms => Some(length.value * root_font_size_px),
        LengthUnit::ViewportWidth => viewport
            .map(|viewport_metrics| length.value * (viewport_metrics.width_px as f32) / 100.0),
        LengthUnit::ViewportHeight => viewport
            .map(|viewport_metrics| length.value * (viewport_metrics.height_px as f32) / 100.0),
    }
}

/// A CSS <length> value with unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Length {
    pub value: f32,
    pub unit: LengthUnit,
}

/// Viewport metrics used to evaluate viewport-relative units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub width_px: u32,
    pub height_px: u32,
}

/// Parse a CSS <length> (§6.2). Supports px/em/rem and unitless zero per spec.
///
/// # Errors
/// Returns `ParseError::UnexpectedToken` when the next token is not a supported `<length>`.
pub fn parse_length(input: &mut Parser) -> Result<Length, ParseError> {
    match input.next_including_whitespace_and_comments() {
        Ok(token) => match token.clone() {
            Token::Dimension { value, unit, .. } => {
                let lower = unit.as_ref().to_ascii_lowercase();
                let unit_kind = match lower.as_str() {
                    "px" => LengthUnit::Pixels,
                    "em" => LengthUnit::Ems,
                    "rem" => LengthUnit::RootEms,
                    "vw" => LengthUnit::ViewportWidth,
                    "vh" => LengthUnit::ViewportHeight,
                    _ => return Err(ParseError::UnexpectedToken),
                };
                Ok(Length {
                    value,
                    unit: unit_kind,
                })
            }
            Token::Number { value: 0.0, .. } => Ok(Length {
                value: 0.0,
                unit: LengthUnit::Pixels,
            }),
            Token::Ident(_)
            | Token::AtKeyword(_)
            | Token::Hash(_)
            | Token::IDHash(_)
            | Token::QuotedString(_)
            | Token::UnquotedUrl(_)
            | Token::Delim(_)
            | Token::Number { .. }
            | Token::Percentage { .. }
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
        Err(_) => Err(ParseError::UnexpectedToken),
    }
}
