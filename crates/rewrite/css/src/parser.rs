//! Streaming CSS parser with rule categorization.

use crate::rule::{CategorizedRules, CssRule, PropertyValue};
use cssparser::{Parser, ParserInput, Token};
use std::collections::HashMap;
use std::sync::mpsc;

/// CSS rule update sent from parser to page.
#[derive(Debug, Clone)]
pub enum CssUpdate {
    /// New rules parsed and categorized
    Rules(CategorizedRules),
}

/// Streaming CSS parser that processes rules incrementally.
pub struct StreamingCssParser {
    /// Accumulated unparsed text
    buffer: String,

    /// Current source order counter
    source_order: usize,

    /// Channel to send parsed rules
    update_tx: mpsc::Sender<CssUpdate>,
}

impl StreamingCssParser {
    /// Create a new streaming parser that sends updates via channel.
    pub fn new(update_tx: mpsc::Sender<CssUpdate>) -> Self {
        Self {
            buffer: String::new(),
            source_order: 0,
            update_tx,
        }
    }

    /// Feed a chunk of CSS text to the parser.
    /// Parses complete rules and sends them via channel.
    pub fn feed(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);

        // Try to parse complete rules from buffer
        let mut newly_parsed = CategorizedRules::default();

        loop {
            // Try to find a complete rule (ends with })
            if let Some(end_pos) = find_rule_end(&self.buffer) {
                let rule_text = self.buffer[..=end_pos].to_string();
                self.buffer.drain(..=end_pos);

                // Parse this complete rule
                if let Some(rule) = self.parse_rule(&rule_text) {
                    let categorized = rule.split_by_category();
                    newly_parsed.merge(categorized);
                    self.source_order += 1;
                }
            } else {
                // No complete rule yet, wait for more input
                break;
            }
        }

        // Send newly parsed rules
        if !newly_parsed.is_empty() {
            let _ = self.update_tx.send(CssUpdate::Rules(newly_parsed));
        }
    }

    /// Finish parsing and send any remaining rules.
    pub fn finish(&mut self) {
        let remaining = self.buffer.trim();
        if remaining.is_empty() {
            return;
        }

        // Try to parse any remaining content
        if let Some(rule) = self.parse_rule(remaining) {
            let categorized = rule.split_by_category();
            let _ = self.update_tx.send(CssUpdate::Rules(categorized));
        }

        self.buffer.clear();
    }

    /// Parse a single CSS rule from text.
    fn parse_rule(&self, rule_text: &str) -> Option<CssRule> {
        let mut input = ParserInput::new(rule_text);
        let mut parser = Parser::new(&mut input);

        parse_qualified_rule(&mut parser, self.source_order).ok()
    }
}

/// Find the end position of a complete CSS rule in the buffer.
/// Returns the position of the closing } that matches the first opening {.
fn find_rule_end(buffer: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_rule = false;

    for (i, ch) in buffer.char_indices() {
        match ch {
            '{' => {
                depth += 1;
                in_rule = true;
            }
            '}' => {
                depth -= 1;
                if in_rule && depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }

    None
}

/// Parse a qualified rule (selector { declarations }).
fn parse_qualified_rule<'i>(
    parser: &mut Parser<'i, '_>,
    source_order: usize,
) -> Result<CssRule, cssparser::ParseError<'i, ()>> {
    parser.skip_whitespace();

    // Collect selector text until we hit {
    let mut selector_parts = Vec::new();

    loop {
        let start = parser.position();
        match parser.next_including_whitespace() {
            Ok(Token::CurlyBracketBlock) => {
                break;
            }
            Ok(_) => {
                let part = parser.slice_from(start);
                selector_parts.push(part);
            }
            Err(_) => {
                return Err(parser.new_error(cssparser::BasicParseErrorKind::EndOfInput));
            }
        }
    }

    let selector_str = selector_parts.join("").trim().to_string();
    let specificity = calculate_specificity(&selector_str);

    // Parse declaration block
    let declarations = parser
        .parse_nested_block(|parser| {
            Ok::<_, cssparser::ParseError<'i, ()>>(parse_declaration_block(parser))
        })
        .unwrap_or_default();

    Ok(CssRule::new(
        selector_str,
        specificity,
        source_order,
        declarations,
    ))
}

/// Parse CSS declarations from a block.
fn parse_declaration_block(parser: &mut Parser<'_, '_>) -> HashMap<String, PropertyValue> {
    let mut declarations = HashMap::new();

    loop {
        parser.skip_whitespace();

        if parser.is_exhausted() {
            break;
        }

        // Try to parse a declaration
        if let Ok(decl) = parse_declaration(parser) {
            declarations.insert(decl.0, decl.1);
        } else {
            // Skip to next declaration or end
            let _ = parser.next();
        }
    }

    declarations
}

/// Parse a single CSS declaration (property: value).
fn parse_declaration<'i>(
    parser: &mut Parser<'i, '_>,
) -> Result<(String, PropertyValue), cssparser::ParseError<'i, ()>> {
    parser.skip_whitespace();

    // Get property name
    let property_name = match parser.next()? {
        Token::Ident(name) => name.to_string(),
        _ => {
            return Err(
                parser.new_error(cssparser::BasicParseErrorKind::UnexpectedToken(
                    Token::Ident("property".into()),
                )),
            );
        }
    };

    parser.skip_whitespace();

    // Expect colon
    match parser.next()? {
        Token::Colon => {}
        _ => {
            return Err(
                parser.new_error(cssparser::BasicParseErrorKind::UnexpectedToken(
                    Token::Colon,
                )),
            );
        }
    }

    parser.skip_whitespace();

    // Collect value tokens until semicolon or end
    let mut value_parts = Vec::new();
    let mut important = false;

    loop {
        let start = parser.position();
        match parser.next_including_whitespace() {
            Ok(Token::Semicolon) | Err(_) => break,
            Ok(Token::Delim('!')) => {
                // Check for !important
                parser.skip_whitespace();
                if let Ok(Token::Ident(ident)) = parser.next() {
                    if ident.eq_ignore_ascii_case("important") {
                        important = true;
                        continue;
                    }
                }
            }
            Ok(_) => {
                let part = parser.slice_from(start);
                value_parts.push(part);
            }
        }
    }

    let value_str = value_parts.join("").trim().to_string();

    Ok((
        property_name,
        PropertyValue {
            value: value_str,
            important,
        },
    ))
}

/// Calculate CSS specificity (a, b, c) for a selector.
fn calculate_specificity(selector: &str) -> (u32, u32, u32) {
    let mut ids = 0u32;
    let mut classes = 0u32;
    let mut elements = 0u32;

    let parts = selector.split(|c: char| c.is_whitespace() || c == '>' || c == '+' || c == '~');

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Count IDs
        ids += part.matches('#').count() as u32;

        // Count classes, attributes, pseudo-classes
        classes += part.matches('.').count() as u32;
        classes += part.matches('[').count() as u32;
        classes += part.matches(':').count() as u32 - part.matches("::").count() as u32;

        // Count elements and pseudo-elements
        elements += part.matches("::").count() as u32;

        // Count element type selectors (rough heuristic)
        if !part.starts_with('#')
            && !part.starts_with('.')
            && !part.starts_with('[')
            && !part.starts_with(':')
        {
            elements += 1;
        }
    }

    (ids, classes, elements)
}
