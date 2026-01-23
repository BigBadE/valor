//! CSS stylesheet parsing and selector matching using cssparser and selectors.

use cssparser::{Parser, ParserInput};
use std::collections::HashMap;

/// A parsed CSS rule with a selector and declarations.
#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: String,
    pub declarations: HashMap<String, String>,
}

/// Parse a CSS stylesheet into rules.
pub fn parse_stylesheet(css: &str) -> Vec<CssRule> {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut rules = Vec::new();

    // Parse top-level rules
    while !parser.is_exhausted() {
        // Skip whitespace
        let _ = parser.skip_whitespace();

        if parser.is_exhausted() {
            break;
        }

        // Try to parse a qualified rule (selector { declarations })
        if let Ok(rule) = parse_qualified_rule(&mut parser) {
            rules.push(rule);
        } else {
            // Skip to next rule on error
            let _ = parser.next();
        }
    }

    rules
}

/// Parse a single qualified rule (selector { declarations })
fn parse_qualified_rule<'i, 't>(
    parser: &mut Parser<'i, 't>,
) -> Result<CssRule, cssparser::ParseError<'i, ()>> {
    // Parse the selector (everything before the {)
    let selector_start = parser.position();

    // Find the opening brace
    loop {
        if parser.is_exhausted() {
            return Err(parser.new_unexpected_token_error(cssparser::Token::Delim('{')));
        }

        let token = parser.next()?;
        if matches!(token, cssparser::Token::CurlyBracketBlock) {
            break;
        }
    }

    let selector_end = parser.position();
    let selector = parser
        .slice_from(selector_start)
        .trim_end_matches('{')
        .trim();

    // Parse the declaration block
    let declarations = parser.parse_nested_block(|parser| parse_declaration_list(parser))?;

    Ok(CssRule {
        selector: selector.to_string(),
        declarations,
    })
}

/// Parse a list of CSS declarations
fn parse_declaration_list<'i, 't>(
    parser: &mut Parser<'i, 't>,
) -> Result<HashMap<String, String>, cssparser::ParseError<'i, ()>> {
    let mut declarations = HashMap::new();

    while !parser.is_exhausted() {
        parser.skip_whitespace();

        if parser.is_exhausted() {
            break;
        }

        // Parse property name
        let property = match parser.next()? {
            cssparser::Token::Ident(name) => name.to_string(),
            _ => continue,
        };

        // Expect colon
        parser.expect_colon()?;

        // Parse value (everything until semicolon or end)
        let value_start = parser.position();
        let mut depth = 0;

        loop {
            if parser.is_exhausted() {
                break;
            }

            match parser.next()? {
                cssparser::Token::Semicolon if depth == 0 => break,
                cssparser::Token::CurlyBracketBlock
                | cssparser::Token::SquareBracketBlock
                | cssparser::Token::ParenthesisBlock => depth += 1,
                _ => {}
            }
        }

        let value_end = parser.position();
        let value = parser
            .slice(value_start..value_end)
            .trim_end_matches(';')
            .trim();

        declarations.insert(property, value.to_string());
    }

    Ok(declarations)
}
