//! CSS Values & Units Level 3 — §9 Colors (subset)
//! Spec: <https://www.w3.org/TR/css-color-3/>

use crate::ParseError;
use cssparser::{ParseError as CssParseError, Parser, Token};

/// Bit count used to duplicate a single hex nibble into a full byte.
const NIBBLE_SHIFT: u32 = 4;

/// A minimal RGBA color representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

/// Convert an ASCII hex digit to its numeric value.
pub const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0' => Some(0),
        b'1' => Some(1),
        b'2' => Some(2),
        b'3' => Some(3),
        b'4' => Some(4),
        b'5' => Some(5),
        b'6' => Some(6),
        b'7' => Some(7),
        b'8' => Some(8),
        b'9' => Some(9),
        b'a' | b'A' => Some(10),
        b'b' | b'B' => Some(11),
        b'c' | b'C' => Some(12),
        b'd' | b'D' => Some(13),
        b'e' | b'E' => Some(14),
        b'f' | b'F' => Some(15),
        _ => None,
    }
}

/// Parse a 3- or 6-digit hex color (e.g., `#abc` or `#aabbcc`).
fn parse_hex_color(text: &str) -> Option<Color> {
    let trimmed = text.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() == 3
        && bytes
            .iter()
            .copied()
            .all(|byte_val| hex_value(byte_val).is_some())
    {
        let r_nibble = bytes.first().and_then(|byte_ref| hex_value(*byte_ref))?;
        let g_nibble = bytes.get(1).and_then(|byte_ref| hex_value(*byte_ref))?;
        let b_nibble = bytes.get(2).and_then(|byte_ref| hex_value(*byte_ref))?;
        // Duplicate nibble (e.g., a -> aa)
        let red_component = r_nibble.wrapping_shl(NIBBLE_SHIFT) | r_nibble;
        let green_component = g_nibble.wrapping_shl(NIBBLE_SHIFT) | g_nibble;
        let blue_component = b_nibble.wrapping_shl(NIBBLE_SHIFT) | b_nibble;
        return Some(Color {
            red: red_component,
            green: green_component,
            blue: blue_component,
            alpha: 255,
        });
    }
    if bytes.len() == 6
        && bytes
            .iter()
            .copied()
            .all(|byte_val| hex_value(byte_val).is_some())
    {
        let r_high = bytes.first().and_then(|byte_ref| hex_value(*byte_ref))?;
        let r_low = bytes.get(1).and_then(|byte_ref| hex_value(*byte_ref))?;
        let g_high = bytes.get(2).and_then(|byte_ref| hex_value(*byte_ref))?;
        let g_low = bytes.get(3).and_then(|byte_ref| hex_value(*byte_ref))?;
        let b_high = bytes.get(4).and_then(|byte_ref| hex_value(*byte_ref))?;
        let b_low = bytes.get(5).and_then(|byte_ref| hex_value(*byte_ref))?;
        let red_component_u32 = u32::from(r_high).wrapping_shl(NIBBLE_SHIFT) | u32::from(r_low);
        let green_component_u32 = u32::from(g_high).wrapping_shl(NIBBLE_SHIFT) | u32::from(g_low);
        let blue_component_u32 = u32::from(b_high).wrapping_shl(NIBBLE_SHIFT) | u32::from(b_low);
        let red_u8 = u8::try_from(red_component_u32).ok()?;
        let green_u8 = u8::try_from(green_component_u32).ok()?;
        let blue_u8 = u8::try_from(blue_component_u32).ok()?;
        return Some(Color {
            red: red_u8,
            green: green_u8,
            blue: blue_u8,
            alpha: 255,
        });
    }
    None
}

/// Map a CSS named color (minimal subset) to an RGBA value.
fn named_color(name: &str) -> Option<Color> {
    match name.to_ascii_lowercase().as_str() {
        "black" => Some(Color {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        }),
        "white" => Some(Color {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 255,
        }),
        "red" => Some(Color {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        }),
        "green" => Some(Color {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }),
        "blue" => Some(Color {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }),
        _ => None,
    }
}

/// Parse `rgb()`/`rgba()` with integer components only (0..=255).
/// Alpha is an optional 4th integer (0..=255).
fn parse_rgb_function(name: &str, input: &mut Parser) -> Option<Color> {
    let lowercase = name.to_ascii_lowercase();
    let mut comps: Vec<u8> = Vec::with_capacity(4);
    while let Ok(token) = input.next_including_whitespace_and_comments() {
        match token.clone() {
            Token::Number { int_value, .. } => {
                if let Some(int_val) = int_value {
                    let bounded = int_val.clamp(i32::from(u8::MIN), i32::from(u8::MAX));
                    if comps.len() < 4
                        && let Ok(component) = u8::try_from(bounded)
                    {
                        comps.push(component);
                    }
                }
            }
            Token::Comma | Token::WhiteSpace(_) | Token::Comment(_) => {}
            Token::CloseParenthesis => break,
            _ => return None,
        }
    }
    if lowercase == "rgb"
        && comps.len() == 3
        && let (Some(red_v), Some(green_v), Some(blue_v)) =
            (comps.first(), comps.get(1), comps.get(2))
    {
        return Some(Color {
            red: *red_v,
            green: *green_v,
            blue: *blue_v,
            alpha: 255,
        });
    }
    if lowercase == "rgba"
        && comps.len() == 4
        && let (Some(red_v), Some(green_v), Some(blue_v), Some(alpha_v)) =
            (comps.first(), comps.get(1), comps.get(2), comps.get(3))
    {
        return Some(Color {
            red: *red_v,
            green: *green_v,
            blue: *blue_v,
            alpha: *alpha_v,
        });
    }
    None
}

/// Parse a CSS <color> (subset).
///
/// Supports hex triplets and sextets, and a minimal set of named colors.
///
/// # Errors
/// Returns `ParseError::UnexpectedToken` for unsupported or malformed input.
pub fn parse_color(input: &mut Parser) -> Result<Color, ParseError> {
    let token_initial = match input.next_including_whitespace_and_comments() {
        Ok(initial) => initial.clone(),
        Err(_) => return Err(ParseError::UnexpectedToken),
    };
    match token_initial {
        Token::Hash(value) => parse_hex_color(value.as_ref()).ok_or(ParseError::UnexpectedToken),
        Token::Ident(name) => named_color(name.as_ref()).ok_or(ParseError::UnexpectedToken),
        Token::Function(name) => {
            let result: Result<Option<Color>, CssParseError<'_, ()>> =
                input.parse_nested_block(|nested| Ok(parse_rgb_function(name.as_ref(), nested)));
            match result {
                Ok(Some(color)) => Ok(color),
                _ => Err(ParseError::UnexpectedToken),
            }
        }
        Token::AtKeyword(_)
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
        | Token::ParenthesisBlock
        | Token::SquareBracketBlock
        | Token::CurlyBracketBlock
        | Token::BadUrl(_)
        | Token::BadString(_)
        | Token::CloseParenthesis
        | Token::CloseSquareBracket
        | Token::CloseCurlyBracket => Err(ParseError::UnexpectedToken),
    }
}
