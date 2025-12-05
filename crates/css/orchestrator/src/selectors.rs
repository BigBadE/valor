//! Selector parsing and specificity utilities for the core CSS engine.
use core::iter::Peekable;
use core::mem::take;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Combinator between two selector parts.
pub(crate) enum Combinator {
    /// Descendant combinator.
    Descendant,
    /// Child combinator.
    Child,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// A simple selector consisting of a tag name, an element id, a list of classes, attribute selectors, or the universal selector.
///
/// Universal selector reference: Selectors Level 4 — 2.2. Universal selector ('*')
/// <https://www.w3.org/TR/selectors-4/#universal-selector>
/// Attribute selector reference: Selectors Level 3 — 6.3. Attribute selectors
/// <https://www.w3.org/TR/selectors-3/#attribute-selectors>
pub(crate) struct SimpleSelector {
    /// Optional tag name, lower-cased, for type selectors.
    tag: Option<String>,
    /// Optional element id, for `#id` selectors.
    element_id: Option<String>,
    /// Class list for `.class` selectors.
    classes: Vec<String>,
    /// Attribute equality selectors: [attr="value"]
    attr_equals: Vec<(String, String)>,
    /// Whether this selector is the universal selector ('*').
    universal: bool,
}

impl SimpleSelector {
    #[inline]
    /// Optional tag name, lower-cased, for type selectors.
    pub(crate) fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }
    #[inline]
    /// Optional element id, for `#id` selectors.
    pub(crate) fn element_id(&self) -> Option<&str> {
        self.element_id.as_deref()
    }
    #[inline]
    /// Class list for `.class` selectors.
    pub(crate) fn classes(&self) -> &[String] {
        &self.classes
    }
    #[inline]
    /// Attribute equality selectors: [attr="value"]
    pub(crate) fn attr_equals_list(&self) -> &[(String, String)] {
        &self.attr_equals
    }
    #[inline]
    /// True if this simple selector is the universal selector ('*').
    pub(crate) const fn is_universal(&self) -> bool {
        self.universal
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// One compound selector part and the combinator to the next (if any).
pub(crate) struct SelectorPart {
    /// The simple selector list (type/id/classes) of this compound.
    sel: SimpleSelector,
    /// The combinator that links this part to the next one.
    combinator_to_next: Option<Combinator>,
}

impl SelectorPart {
    #[inline]
    /// Access the simple selector of this part.
    pub(crate) const fn sel(&self) -> &SimpleSelector {
        &self.sel
    }
    #[inline]
    /// The combinator to the next part, if present.
    pub(crate) fn combinator_to_next(&self) -> Option<Combinator> {
        self.combinator_to_next.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A full selector consisting of multiple parts.
pub(crate) struct Selector(Vec<SelectorPart>);

impl Selector {
    #[inline]
    /// Number of parts in this selector.
    pub(crate) const fn len(&self) -> usize {
        self.0.len()
    }
    #[inline]
    /// Return the selector part at `index`, if present.
    pub(crate) fn part(&self, index: usize) -> Option<&SelectorPart> {
        self.0.get(index)
    }
}

/// Specificity represented as (ids, classes, tags)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Specificity(pub u32, pub u32, pub u32);

/// Consume an identifier from a character iterator.
fn consume_ident<I>(chars: &mut Peekable<I>, allow_underscore: bool) -> String
where
    I: Iterator<Item = char>,
{
    let mut out = String::new();
    while let Some(&character) = chars.peek() {
        let is_acceptable = character.is_alphanumeric()
            || character == '-'
            || (allow_underscore && character == '_');
        if !is_acceptable {
            break;
        }
        out.push(character);
        chars.next();
    }
    out
}

/// Push the current simple selector into `parts` and reset `current`, attaching `combinator`.
fn commit_current_part(
    parts: &mut Vec<SelectorPart>,
    current: &mut SimpleSelector,
    combinator: Combinator,
) {
    parts.push(SelectorPart {
        sel: SimpleSelector {
            tag: current.tag.take(),
            element_id: current.element_id.take(),
            classes: take(&mut current.classes),
            attr_equals: take(&mut current.attr_equals),
            universal: take(&mut current.universal),
        },
        combinator_to_next: Some(combinator),
    });
}

/// Compute the specificity (ids, classes, tags) for a parsed selector.
/// Per CSS Selectors Level 3 §9: Attribute selectors count as classes
pub(crate) fn compute_specificity(selector: &Selector) -> Specificity {
    let mut ids = 0u32;
    let mut classes = 0u32;
    let mut tags = 0u32;
    for part in &selector.0 {
        if part.sel.element_id.is_some() {
            ids = ids.saturating_add(1);
        }
        if !part.sel.classes.is_empty() {
            let len_u32 = u32::try_from(part.sel.classes.len()).unwrap_or(u32::MAX);
            classes = classes.saturating_add(len_u32);
        }
        // Attribute selectors count as classes per spec
        if !part.sel.attr_equals.is_empty() {
            let len_u32 = u32::try_from(part.sel.attr_equals.len()).unwrap_or(u32::MAX);
            classes = classes.saturating_add(len_u32);
        }
        if part.sel.tag.is_some() {
            tags = tags.saturating_add(1);
        }
    }
    Specificity(ids, classes, tags)
}

/// Check if a `SimpleSelector` has any meaningful content.
fn has_content(sel: &SimpleSelector) -> bool {
    sel.universal
        || sel.tag.is_some()
        || sel.element_id.is_some()
        || !sel.classes.is_empty()
        || !sel.attr_equals.is_empty()
}

/// Skip over balanced parentheses in a character iterator.
/// Consumes the opening '(' and all characters until the matching closing ')'.
fn skip_balanced_parens<I>(chars: &mut Peekable<I>)
where
    I: Iterator<Item = char>,
{
    chars.next(); // Consume opening '('
    let mut depth = 1;
    while let Some(&character) = chars.peek() {
        chars.next();
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
    }
}

/// Parse an attribute value (quoted or unquoted) from a character iterator.
fn parse_attr_value<I>(chars: &mut Peekable<I>) -> String
where
    I: Iterator<Item = char>,
{
    let Some(quote) = chars.peek().copied() else {
        return String::new();
    };

    if quote == '"' || quote == '\'' {
        chars.next(); // consume opening quote
        let mut value = String::new();
        while let Some(&character) = chars.peek() {
            if character == quote {
                chars.next(); // consume closing quote
                break;
            }
            value.push(character);
            chars.next();
        }
        value
    } else {
        consume_ident(chars, true)
    }
}

/// Parse an attribute selector like `[attr="value"]` from a character iterator.
/// Assumes the opening '[' has already been consumed.
fn parse_attribute_selector<I>(chars: &mut Peekable<I>) -> Vec<(String, String)>
where
    I: Iterator<Item = char>,
{
    // Skip whitespace
    while chars.peek().is_some_and(char::is_ascii_whitespace) {
        chars.next();
    }
    let attr_name = consume_ident(chars, true);
    // Skip whitespace
    while chars.peek().is_some_and(char::is_ascii_whitespace) {
        chars.next();
    }
    // Check for '='
    let mut result = Vec::new();
    if chars.peek().copied() == Some('=') {
        chars.next();
        // Skip whitespace
        while chars.peek().is_some_and(char::is_ascii_whitespace) {
            chars.next();
        }
        // Parse value (quoted or unquoted)
        let attr_value = parse_attr_value(chars);
        result.push((attr_name, attr_value));
    }
    // Skip to closing ']'
    while chars.peek().is_some_and(|&character| character != ']') {
        chars.next();
    }
    if chars.peek().copied() == Some(']') {
        chars.next();
    }
    result
}

/// Process a single character in selector parsing.
/// Returns `None` if the selector should be discarded (e.g., pseudo-classes).
fn process_selector_char<I>(
    character: char,
    chars: &mut Peekable<I>,
    current: &mut SimpleSelector,
    parts: &mut Vec<SelectorPart>,
    next_combinator: &mut Option<Combinator>,
) -> Option<()>
where
    I: Iterator<Item = char>,
{
    match character {
        '>' => {
            chars.next();
            if has_content(current) {
                commit_current_part(parts, current, Combinator::Child);
                *next_combinator = None;
            } else {
                *next_combinator = Some(Combinator::Child);
            }
        }
        '*' => {
            chars.next();
            current.universal = true;
        }
        ':' => {
            // Pseudo-class or pseudo-element - discard selector
            chars.next();
            if chars.peek().copied() == Some(':') {
                chars.next();
            }
            let _pseudo_name = consume_ident(chars, true);
            if chars.peek().copied() == Some('(') {
                skip_balanced_parens(chars);
            }
            return None;
        }
        '#' => {
            chars.next();
            current.element_id = Some(consume_ident(chars, true));
        }
        '.' => {
            chars.next();
            current.classes.push(consume_ident(chars, true));
        }
        '[' => {
            chars.next();
            let attrs = parse_attribute_selector(chars);
            current.attr_equals.extend(attrs);
        }
        character if character.is_alphanumeric() => {
            current.tag = Some(consume_ident(chars, false));
        }
        _ => {
            chars.next();
        }
    }
    Some(())
}

/// Parse a single selector string into a `Selector`.
fn parse_single_selector(selector_str: &str) -> Option<Selector> {
    let mut chars = selector_str.trim().chars().peekable();
    let mut parts: Vec<SelectorPart> = Vec::new();
    let mut current = SimpleSelector::default();
    let mut next_combinator: Option<Combinator> = None;
    let mut saw_whitespace = false;

    loop {
        // Consume whitespace as a descendant combinator boundary.
        while chars.peek().is_some_and(char::is_ascii_whitespace) {
            saw_whitespace = true;
            chars.next();
        }
        if saw_whitespace {
            if has_content(&current) {
                commit_current_part(&mut parts, &mut current, Combinator::Descendant);
                next_combinator = None;
            } else {
                next_combinator = Some(Combinator::Descendant);
            }
        }
        let Some(character) = chars.peek().copied() else {
            break;
        };
        process_selector_char(
            character,
            &mut chars,
            &mut current,
            &mut parts,
            &mut next_combinator,
        )?;
    }
    if has_content(&current) {
        parts.push(SelectorPart {
            sel: current,
            combinator_to_next: next_combinator.take(),
        });
        if let Some(last) = parts.last_mut() {
            last.combinator_to_next = None;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(Selector(parts))
    }
}

/// Parse a selector list separated by commas into a vector of `Selector`s.
pub(crate) fn parse_selector_list(input: &str) -> Vec<Selector> {
    input.split(',').filter_map(parse_single_selector).collect()
}
