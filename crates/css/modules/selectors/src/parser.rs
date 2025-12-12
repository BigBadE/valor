//! CSS selector parsing.
//! Spec: <https://www.w3.org/TR/selectors-3/>

use crate::{Combinator, ComplexSelector, CompoundSelector, SelectorList, SimpleSelector};
use core::mem::take;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Internal tokenizer token kinds.
pub enum Tok {
    /// A combinator token like child/adjacent/general sibling.
    Combinator(Combinator),
    /// Whitespace that implies a descendant combinator.
    DescendantWS,
    /// A simple selector token (type, class, id, attribute, universal).
    Simple(SimpleSelector),
}

/// Tokenizer over a selector string.
pub struct SelectorTokenizer {
    /// Underlying owned bytes for the selector.
    input_bytes: Vec<u8>,
    /// Current cursor index into `input_bytes`.
    index: usize,
    /// Whether we should emit a descendant whitespace token on `next()` call.
    pending_whitespace: bool,
}

impl SelectorTokenizer {
    /// Construct a tokenizer from input.
    #[inline]
    pub(crate) fn new(input: &str) -> Self {
        Self {
            input_bytes: input.as_bytes().to_vec(),
            index: 0,
            pending_whitespace: false,
        }
    }

    /// Return the next selector token, if any.
    #[inline]
    pub(crate) fn next(&mut self) -> Option<Tok> {
        if self.pending_whitespace {
            self.pending_whitespace = false;
            return Some(Tok::DescendantWS);
        }
        self.skip_whitespace_descendant();
        if let Some(&current) = self.input_bytes.get(self.index) {
            match current {
                b'*' => {
                    self.index = self.index.saturating_add(1);
                    Some(Tok::Simple(SimpleSelector::Universal))
                }
                b'.' => Some(self.consume_class()),
                b'#' => Some(self.consume_id()),
                b'[' => Some(self.consume_attr()),
                b'>' => {
                    self.index = self.index.saturating_add(1);
                    Some(Tok::Combinator(Combinator::Child))
                }
                b'+' => {
                    self.index = self.index.saturating_add(1);
                    Some(Tok::Combinator(Combinator::AdjacentSibling))
                }
                b'~' => {
                    self.index = self.index.saturating_add(1);
                    Some(Tok::Combinator(Combinator::GeneralSibling))
                }
                _ => Some(self.consume_type()),
            }
        } else {
            None
        }
    }

    /// Skip whitespace and mark that a descendant combinator should be emitted next.
    #[inline]
    fn skip_whitespace_descendant(&mut self) {
        let mut saw = false;
        while let Some(&byte) = self.input_bytes.get(self.index) {
            if byte.is_ascii_whitespace() {
                saw = true;
                self.index = self.index.saturating_add(1);
            } else {
                break;
            }
        }
        if saw {
            self.pending_whitespace = true;
        }
    }

    /// Consume an identifier consisting of ASCII alphanumerics, '-' and '_', lowercased.
    #[inline]
    fn consume_ident(&mut self) -> String {
        let start = self.index;
        while let Some(&byte) = self.input_bytes.get(self.index) {
            if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
                self.index = self.index.saturating_add(1);
            } else {
                break;
            }
        }
        let slice = self.input_bytes.get(start..self.index).unwrap_or(&[]);
        String::from_utf8_lossy(slice).to_ascii_lowercase()
    }

    /// Parse a type selector identifier into a `SimpleSelector::Type`.
    #[inline]
    fn consume_type(&mut self) -> Tok {
        let ident = self.consume_ident();
        Tok::Simple(SimpleSelector::Type(ident))
    }

    /// Parse a class selector following '.' into `SimpleSelector::Class`.
    #[inline]
    fn consume_class(&mut self) -> Tok {
        // skip '.'
        self.index = self.index.saturating_add(1);
        let ident = self.consume_ident();
        Tok::Simple(SimpleSelector::Class(ident))
    }

    /// Parse an id selector following '#' into `SimpleSelector::Id`.
    #[inline]
    fn consume_id(&mut self) -> Tok {
        // skip '#'
        self.index = self.index.saturating_add(1);
        let ident = self.consume_ident();
        Tok::Simple(SimpleSelector::IdSelector(ident))
    }

    /// Parse an attribute selector prelude, supporting `[name]` and `[name=value]` (quoted or unquoted).
    #[inline]
    fn consume_attr(&mut self) -> Tok {
        // skip '['
        self.index = self.index.saturating_add(1);
        self.skip_spaces();
        let name = self.consume_ident();
        self.skip_spaces();
        let value = if self
            .input_bytes
            .get(self.index)
            .is_some_and(|&byte| byte == b'=')
        {
            self.index = self.index.saturating_add(1);
            self.skip_spaces();
            if self
                .input_bytes
                .get(self.index)
                .is_some_and(|&byte| byte == b'"' || byte == b'\'')
            {
                let quote = *self.input_bytes.get(self.index).unwrap_or(&b'"');
                self.index = self.index.saturating_add(1);
                self.consume_quoted_attr_value(quote)
            } else {
                self.consume_unquoted_attr_value()
            }
        } else {
            String::new()
        };
        self.skip_spaces();
        if self
            .input_bytes
            .get(self.index)
            .is_some_and(|&byte| byte == b']')
        {
            self.index = self.index.saturating_add(1);
        }
        Tok::Simple(SimpleSelector::AttrEquals { name, value })
    }

    /// Consume an unquoted attribute value until whitespace or a closing bracket.
    #[inline]
    fn consume_unquoted_attr_value(&mut self) -> String {
        let start = self.index;
        while let Some(&byte) = self.input_bytes.get(self.index) {
            if byte.is_ascii_whitespace() || byte == b']' {
                break;
            }
            self.index = self.index.saturating_add(1);
        }
        let slice = self.input_bytes.get(start..self.index).unwrap_or(&[]);
        String::from_utf8_lossy(slice).to_string()
    }

    /// Consume a quoted attribute value until the matching quote byte.
    #[inline]
    fn consume_quoted_attr_value(&mut self, quote: u8) -> String {
        let start = self.index;
        while matches!(self.input_bytes.get(self.index), Some(&byte) if byte != quote) {
            self.index = self.index.saturating_add(1);
        }
        let slice = self.input_bytes.get(start..self.index).unwrap_or(&[]);
        let out = String::from_utf8_lossy(slice).to_string();
        if self.input_bytes.get(self.index).is_some() {
            self.index = self.index.saturating_add(1);
        }
        out
    }

    /// Skip ASCII whitespace.
    #[inline]
    fn skip_spaces(&mut self) {
        while matches!(self.input_bytes.get(self.index), Some(byte) if byte.is_ascii_whitespace()) {
            self.index = self.index.saturating_add(1);
        }
    }
}

/// Parse a selector list from CSS text.
/// Spec: Section 3, 4, 5–8, 11
pub fn parse_selector_list(input: &str) -> SelectorList {
    let mut list = SelectorList::default();
    for part in input.split(',') {
        let sel = parse_complex_selector(part.trim());
        if !sel.first.simples.is_empty() || !sel.rest.is_empty() {
            list.selectors.push(sel);
        }
    }
    list
}

/// Parse one complex selector (very permissive, minimal error handling).
/// Spec: Section 11 — Combinators; Section 5–8 — simple selectors
///
/// # Panics
/// Never panics.
pub fn parse_complex_selector(input: &str) -> ComplexSelector {
    let mut tokens = SelectorTokenizer::new(input);
    let mut current = CompoundSelector::default();
    let mut first = None;
    let mut rest: Vec<(Combinator, CompoundSelector)> = Vec::new();
    let mut pending_combinator: Option<Combinator> = None;

    while let Some(token) = tokens.next() {
        match token {
            Tok::Combinator(comb) => {
                if first.is_none() {
                    first = Some(take(&mut current));
                } else {
                    rest.push((
                        pending_combinator.unwrap_or(Combinator::Descendant),
                        take(&mut current),
                    ));
                }
                pending_combinator = Some(comb);
            }
            Tok::DescendantWS => {
                // Whitespace can imply descendant combinator if a non-whitespace token follows later
                if pending_combinator.is_none() {
                    pending_combinator = Some(Combinator::Descendant);
                }
            }
            Tok::Simple(simple) => {
                if let Some(prev_comb) = pending_combinator.take()
                    && !current.simples.is_empty()
                {
                    if first.is_none() {
                        first = Some(take(&mut current));
                    } else {
                        rest.push((prev_comb, take(&mut current)));
                    }
                }
                current.simples.push(simple);
            }
        }
    }

    if first.is_none() {
        first = Some(current);
    } else if !current.simples.is_empty() {
        if let Some(prev_comb) = pending_combinator.take() {
            rest.push((prev_comb, current));
        } else {
            rest.push((Combinator::Descendant, current));
        }
    }

    ComplexSelector {
        first: first.unwrap_or_default(),
        rest,
    }
}
