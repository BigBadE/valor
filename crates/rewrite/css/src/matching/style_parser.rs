//! Parse CSS from stylesheets and populate StyleSheets.

use super::matcher::calculate_specificity;
use super::stylesheet::{StyleRule, StyleSheets};
use crate::value::CssValue;
use cssparser::{Parser, ParserInput, Token};
use std::collections::HashMap;

/// Parse CSS text into a StyleSheets object.
pub fn parse_css(css_text: &str) -> StyleSheets {
    let mut input = ParserInput::new(css_text);
    let mut parser = Parser::new(&mut input);

    let mut rules = Vec::new();
    let mut source_order = 0;

    // Parse all rules in the stylesheet
    loop {
        parser.skip_whitespace();

        if parser.is_exhausted() {
            break;
        }

        // Try to parse a qualified rule
        match parse_qualified_rule(&mut parser, source_order) {
            Ok(rule) => {
                rules.push(rule);
                source_order += 1;
            }
            Err(_) => {
                // Skip this rule and continue
                continue;
            }
        }
    }

    StyleSheets { rules }
}

/// Parse a qualified rule (selector { declarations })
fn parse_qualified_rule<'i>(
    parser: &mut Parser<'i, '_>,
    source_order: usize,
) -> Result<StyleRule, cssparser::ParseError<'i, ()>> {
    // Collect selector text by consuming tokens until {
    let mut selector_parts = Vec::new();

    loop {
        let start = parser.position();
        match parser.next_including_whitespace() {
            Ok(Token::CurlyBracketBlock) => {
                // Found the opening brace - selector is complete
                break;
            }
            Ok(_) => {
                // Part of the selector
                let part = parser.slice_from(start);
                selector_parts.push(part);
            }
            Err(_) => {
                return Err(parser.new_error(cssparser::BasicParseErrorKind::EndOfInput));
            }
        }
    }

    let selector_str = selector_parts.join("").trim().to_string();

    // Calculate specificity
    let specificity = calculate_specificity(&selector_str);

    // Parse the declaration block (we're positioned right after the { token)
    let (declarations, important_declarations) = parser
        .parse_nested_block(|parser| {
            Ok::<_, cssparser::ParseError<'i, ()>>(parse_declaration_block(parser))
        })
        .unwrap_or_default();

    Ok(StyleRule::with_important(
        selector_str,
        declarations,
        important_declarations,
        specificity,
        source_order,
    ))
}

/// Parse CSS declarations from a block, returning (normal_decls, important_decls)
fn parse_declaration_block(
    parser: &mut Parser<'_, '_>,
) -> (HashMap<String, CssValue>, HashMap<String, CssValue>) {
    let mut declarations = HashMap::new();
    let mut important_declarations = HashMap::new();

    loop {
        parser.skip_whitespace();

        if parser.is_exhausted() {
            break;
        }

        // Try to parse property name
        let property = match parser.expect_ident() {
            Ok(prop) => prop.to_string(),
            Err(_) => break,
        };

        // Expect colon
        if parser.expect_colon().is_err() {
            break;
        }

        // Collect value tokens until semicolon or end
        let (value_str, is_important) = parse_value_with_important(parser);

        // Parse the value into a CssValue
        if let Some(css_value) = parse_css_value(&value_str) {
            // Expand shorthand properties
            let expanded = expand_shorthand(&property, css_value);

            for (prop_name, prop_value) in expanded {
                if is_important {
                    important_declarations.insert(prop_name, prop_value);
                } else {
                    declarations.insert(prop_name, prop_value);
                }
            }
        }

        // Skip optional semicolon
        let _ = parser.try_parse(|p| p.expect_semicolon());
    }

    (declarations, important_declarations)
}

/// Parse value tokens into a string, checking for !important
fn parse_value_with_important(parser: &mut Parser<'_, '_>) -> (String, bool) {
    let start = parser.position();
    let mut depth = 0;

    // Consume tokens until semicolon (at depth 0) or end of block
    loop {
        match parser.next_including_whitespace() {
            Ok(Token::Semicolon) if depth == 0 => {
                // Semicolon ends the value - get slice and check for !important
                let end = parser.position();
                let value_str = parser
                    .slice(start..end)
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                let (cleaned_value, is_important) = extract_important(&value_str);
                return (cleaned_value, is_important);
            }
            Ok(Token::ParenthesisBlock | Token::Function(_)) => depth += 1,
            Ok(Token::CloseParenthesis) if depth > 0 => depth -= 1,
            Ok(_) => {
                // Continue collecting tokens
            }
            Err(_) => {
                // End of input - return what we have
                let end = parser.position();
                let value_str = parser.slice(start..end).trim().to_string();
                let (cleaned_value, is_important) = extract_important(&value_str);
                return (cleaned_value, is_important);
            }
        }
    }
}

/// Extract !important from a value string
fn extract_important(value_str: &str) -> (String, bool) {
    // Check if value ends with !important
    if value_str.ends_with("!important") {
        let cleaned = value_str.trim_end_matches("!important").trim().to_string();
        (cleaned, true)
    } else if value_str.ends_with("! important") {
        let cleaned = value_str.trim_end_matches("! important").trim().to_string();
        (cleaned, true)
    } else {
        (value_str.to_string(), false)
    }
}

/// Parse a CSS value string into a CssValue
fn parse_css_value(value_str: &str) -> Option<CssValue> {
    use crate::storage::parser::parse_value;
    parse_value(value_str)
}

/// Expand CSS shorthand properties into longhand properties.
/// Returns a vector of (property_name, value) pairs.
fn expand_shorthand(property: &str, value: CssValue) -> Vec<(String, CssValue)> {
    match property {
        "margin" => expand_box_shorthand("margin", value),
        "padding" => expand_box_shorthand("padding", value),
        "border-width" => expand_box_shorthand("border", value),
        _ => {
            // Not a shorthand - return as-is
            vec![(property.to_string(), value)]
        }
    }
}

/// Expand box model shorthands (margin, padding, border-width).
/// Handles 1-4 value syntax.
fn expand_box_shorthand(prefix: &str, value: CssValue) -> Vec<(String, CssValue)> {
    // For single keyword values (like "auto"), apply to all sides
    match value {
        CssValue::Keyword(_) => {
            vec![
                (format!("{}-top", prefix), value.clone()),
                (format!("{}-right", prefix), value.clone()),
                (format!("{}-bottom", prefix), value.clone()),
                (format!("{}-left", prefix), value),
            ]
        }
        CssValue::Length(_) | CssValue::Percentage(_) | CssValue::Number(_) => {
            // Single length value - apply to all sides
            vec![
                (format!("{}-top", prefix), value.clone()),
                (format!("{}-right", prefix), value.clone()),
                (format!("{}-bottom", prefix), value.clone()),
                (format!("{}-left", prefix), value),
            ]
        }
        _ => {
            // Complex values not supported yet - treat as single value
            vec![
                (format!("{}-top", prefix), value.clone()),
                (format!("{}-right", prefix), value.clone()),
                (format!("{}-bottom", prefix), value.clone()),
                (format!("{}-left", prefix), value),
            ]
        }
    }
}
