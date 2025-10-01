//! CSS Style Attributes — style="..." attribute processing.
//! Spec: <https://www.w3.org/TR/css-style-attr/>

#![forbid(unsafe_code)]

use std::collections::HashMap;

/// A single CSS declaration parsed from a style attribute.
///
/// Spec: <https://www.w3.org/TR/css-style-attr/#interpreting>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// Property name normalized to ASCII lowercase as per CSS case-insensitivity.
    pub property: String,
    /// Raw value slice trimmed of surrounding ASCII whitespace. May contain spaces.
    pub value: String,
}

/// Parse the value of a `style` attribute into a list of declarations.
///
/// This performs a minimal, resilient parse suitable for early integration and tests:
/// - Splits on semicolons (`;`) into declaration items.
/// - For each item, splits on the first colon (`:`) into property and value.
/// - Trims ASCII whitespace and lowercases the property name.
/// - Skips empty or invalid items (no colon, empty property, or empty value after trimming).
///
/// Note: This does not implement full tokenization, !important handling, or error recovery
/// beyond skipping invalid entries. It is intentionally small and can be replaced by a
/// tokenizer-backed implementation in the css `syntax` module later.
///
/// Spec: <https://www.w3.org/TR/css-style-attr/#interpreting>
pub fn parse_style_attribute(input: &str) -> Vec<Declaration> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Declaration> = Vec::new();
    for raw_item in input.split(';') {
        let item = raw_item.trim_matches(is_ascii_whitespace);
        if item.is_empty() {
            continue;
        }
        let Some((raw_prop, raw_value)) = item.split_once(':') else {
            continue;
        };
        let property_text = raw_prop.trim_matches(is_ascii_whitespace);
        let value_text = raw_value.trim_matches(is_ascii_whitespace);
        if property_text.is_empty() || value_text.is_empty() {
            continue;
        }
        out.push(Declaration {
            property: to_ascii_lowercase(property_text),
            value: value_text.to_owned(),
        });
    }
    out
}

/// Convenience: parse into a map keyed by property name.
///
/// If a property appears multiple times, the last one wins, matching standard
/// source-order behavior for duplicate declarations within the same block.
///
/// Spec: <https://www.w3.org/TR/css-style-attr/#interpreting>
pub fn parse_style_attribute_into_map(input: &str) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for decl in parse_style_attribute(input) {
        map.insert(decl.property, decl.value);
    }
    map
}

/// ASCII whitespace per CSS Syntax (TAB, LF, FF, CR, SPACE).
///
/// Spec: <https://www.w3.org/TR/css-syntax-3/#whitespace>
const fn is_ascii_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}' | '\u{000A}' | '\u{000C}' | '\u{000D}' | '\u{0020}'
    )
}

/// Lowercase an ASCII identifier without allocating when already lowercase.
///
/// This keeps behavior simple and predictable for property names.
///
/// Spec: <https://www.w3.org/TR/css-style-attr/#interpreting>
fn to_ascii_lowercase(text: &str) -> String {
    // Avoid allocation if already lowercase ASCII
    let mut needs_lowercase = false;
    for character in text.chars() {
        if character.is_ascii_uppercase() {
            needs_lowercase = true;
            break;
        }
    }
    if !needs_lowercase {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        out.push(character.to_ascii_lowercase());
    }
    out
}

/// Normalize and filter a raw attribute string, keeping only the last occurrence
/// of each property. This is a simple helper useful for tests.
///
/// Spec: <https://www.w3.org/TR/css-style-attr/#interpreting>
pub fn normalize_style_attribute(input: &str) -> Vec<Declaration> {
    let mut last_index_for_property: HashMap<String, usize> = HashMap::new();
    let declarations = parse_style_attribute(input);
    for (index, decl_item) in declarations.iter().enumerate() {
        last_index_for_property.insert(decl_item.property.clone(), index);
    }
    // Retain only the last index for each property
    declarations
        .into_iter()
        .enumerate()
        .filter_map(
            |(index, decl_item)| match last_index_for_property.get(&decl_item.property) {
                Some(&last_index) if last_index == index => Some(decl_item),
                _ => None,
            },
        )
        .collect()
}
