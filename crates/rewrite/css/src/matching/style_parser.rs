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
    let declarations = parser
        .parse_nested_block(|parser| {
            Ok::<_, cssparser::ParseError<'i, ()>>(parse_declaration_block(parser))
        })
        .unwrap_or_default();

    Ok(StyleRule::new(
        selector_str,
        declarations,
        specificity,
        source_order,
    ))
}

/// Parse CSS declarations from a block
fn parse_declaration_block(parser: &mut Parser<'_, '_>) -> HashMap<String, CssValue> {
    let mut declarations = HashMap::new();

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
        let value_str = parse_value_string(parser);

        // Parse the value into a CssValue
        if let Some(css_value) = parse_css_value(&value_str) {
            declarations.insert(property, css_value);
        }

        // Skip optional semicolon
        let _ = parser.try_parse(|p| p.expect_semicolon());
    }

    declarations
}

/// Parse value tokens into a string
fn parse_value_string(parser: &mut Parser<'_, '_>) -> String {
    let start = parser.position();
    let mut depth = 0;

    // Consume tokens until semicolon (at depth 0) or end of block
    loop {
        match parser.next_including_whitespace() {
            Ok(Token::Semicolon) if depth == 0 => {
                // Semicolon ends the value - get slice and return
                let end = parser.position();
                return parser
                    .slice(start..end)
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
            }
            Ok(Token::ParenthesisBlock | Token::Function(_)) => depth += 1,
            Ok(Token::CloseParenthesis) if depth > 0 => depth -= 1,
            Ok(_) => {
                // Continue collecting tokens
            }
            Err(_) => {
                // End of input - return what we have
                let end = parser.position();
                return parser.slice(start..end).trim().to_string();
            }
        }
    }
}

/// Parse a CSS value string into a CssValue
fn parse_css_value(value_str: &str) -> Option<CssValue> {
    use crate::storage::parser::parse_value;
    parse_value(value_str)
}
