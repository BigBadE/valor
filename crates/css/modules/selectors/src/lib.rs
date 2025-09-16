//! Selectors Level 3 — Element matching and specificity.
//! Spec: <https://www.w3.org/TR/selectors-3/>
//!
//! This module implements a minimal subset needed for Speedometer:
//! - Type, class, id, and attribute equals selectors
//! - Combinators: descendant, child, adjacent sibling, general sibling
//! - Specificity calculation
//! - A simple match cache that can be invalidated on element changes
//!
//! Each function includes a reference to its corresponding section in the spec.

use core::hash::{Hash, Hasher as _};
use core::mem::take;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;

/// An adapter that abstracts DOM access for selector matching.
/// Implement this for your DOM layer.
///
/// Spec references:
/// - Section 3: Selectors overview and element matching
pub trait ElementAdapter {
    type Handle: Copy + Eq;

    /// Unique, stable key for caching per element.
    /// Spec: Section 3 — used only for caching purposes here.
    fn unique_key(&self, element: Self::Handle) -> u64;

    /// Parent element if any.
    /// Spec: Section 11 — Combinators (for tree relationships)
    fn parent(&self, element: Self::Handle) -> Option<Self::Handle>;

    /// Previous sibling element (skip non-elements if your DOM has mixed nodes).
    /// Spec: Section 11 — Sibling combinators
    fn previous_sibling_element(&self, element: Self::Handle) -> Option<Self::Handle>;

    /// Tag name in ASCII lowercase (per HTML parsing conventions).
    /// Spec: Section 5 — Type selectors
    fn tag_name(&self, element: Self::Handle) -> &str;

    /// Returns Some(id) if the element has an id attribute, else None.
    /// Spec: Section 7 — ID selectors
    fn element_id(&self, element: Self::Handle) -> Option<&str>;

    /// True if the element has the given class token.
    /// Spec: Section 6 — Class selectors
    fn has_class(&self, element: Self::Handle, class: &str) -> bool;

    /// Returns the attribute value if present.
    /// Spec: Section 8 — Attribute selectors
    fn attr(&self, element: Self::Handle, name: &str) -> Option<&str>;
}

/// Simple selectors (subset).
/// Spec: Section 5, 6, 7, 8
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SimpleSelector {
    /// Spec: Section 5 — Type selectors
    Type(String),
    /// Spec: Section 6 — Class selectors
    Class(String),
    /// Spec: Section 7 — ID selectors
    IdSelector(String),
    /// Spec: Section 8 — Attribute selectors [attr=value]
    AttrEquals { name: String, value: String },
    /// Universal selector '*'. For simplicity we parse but it's a no-op match.
    /// Spec: Section 5 — Universal selector
    Universal,
}

/// A compound selector is a sequence of simple selectors (no combinators).
/// Spec: Section 5 — Simple selector sequences
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CompoundSelector {
    pub simples: Vec<SimpleSelector>,
}

/// Combinators between compounds.
/// Spec: Section 11 — Combinators
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

/// A complex selector is one or more compounds separated by combinators.
/// Spec: Section 3, 11
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ComplexSelector {
    pub first: CompoundSelector,
    pub rest: Vec<(Combinator, CompoundSelector)>,
}

/// A selector list separated by commas.
/// Spec: Section 4 — Groups of selectors
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SelectorList {
    pub selectors: Vec<ComplexSelector>,
}

/// Specificity triple (a, b, c).
/// Spec: Section 13 — Calculating a selector's specificity
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Specificity(pub u16, pub u16, pub u16);

/// Parse a selector list from CSS text.
/// Spec: Section 3, 4, 5–8, 11
#[inline]
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
#[inline]
pub fn parse_complex_selector(input: &str) -> ComplexSelector {
    let mut tokens = tokenize_selector(input);
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

#[derive(Clone, Debug, PartialEq, Eq)]
/// Internal tokenizer token kinds.
enum Tok {
    /// A combinator token like child/adjacent/general sibling.
    Combinator(Combinator),
    /// Whitespace that implies a descendant combinator.
    DescendantWS,
    /// A simple selector token (type, class, id, attribute, universal).
    Simple(SimpleSelector),
}

/// Tokenize a selector into a stream of tokens that our minimal parser understands.
/// Spec: Section 3, 5–8, 11
#[inline]
fn tokenize_selector(input: &str) -> SelectorTokenizer {
    SelectorTokenizer::new(input)
}

/// Tokenizer over a selector string.
struct SelectorTokenizer {
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
    fn new(input: &str) -> Self {
        Self {
            input_bytes: input.as_bytes().to_vec(),
            index: 0,
            pending_whitespace: false,
        }
    }

    /// Return the next selector token, if any.
    #[inline]
    fn next(&mut self) -> Option<Tok> {
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

/// Compute the specificity of a compound selector.
/// Spec: Section 13 — Specificity (a, b, c)
#[inline]
pub fn specificity_of_compound(compound: &CompoundSelector) -> Specificity {
    let mut id_count = 0u16;
    let mut class_attr_count = 0u16;
    let mut type_count = 0u16;
    for simple in compound.simples.iter().cloned() {
        match simple {
            SimpleSelector::IdSelector(_) => {
                id_count = id_count.saturating_add(1);
            }
            SimpleSelector::Class(_) | SimpleSelector::AttrEquals { .. } => {
                class_attr_count = class_attr_count.saturating_add(1);
            }
            SimpleSelector::Type(name) => {
                if name.as_str() != "*" {
                    type_count = type_count.saturating_add(1);
                }
            }
            SimpleSelector::Universal => {}
        }
    }
    Specificity(id_count, class_attr_count, type_count)
}

/// Compute the specificity of a complex selector (sum of its compounds).
/// Spec: Section 13 — Specificity accumulation
#[inline]
pub fn specificity_of_complex(sel: &ComplexSelector) -> Specificity {
    let mut spec_total = specificity_of_compound(&sel.first);
    for pair in &sel.rest {
        let compound = &pair.1;
        let spec_add = specificity_of_compound(compound);
        spec_total.0 = spec_total.0.saturating_add(spec_add.0);
        spec_total.1 = spec_total.1.saturating_add(spec_add.1);
        spec_total.2 = spec_total.2.saturating_add(spec_add.2);
    }
    spec_total
}

/// Match a selector list against an element.
/// Spec: Section 3, 4
#[inline]
pub fn matches_selector_list<A: ElementAdapter>(
    adapter: &A,
    element: A::Handle,
    list: &SelectorList,
) -> bool {
    list.selectors
        .iter()
        .any(|selector_item| matches_complex(adapter, element, selector_item))
}

/// Match a complex selector against an element.
/// Spec: Section 3, 11 — Right-to-left matching strategy
#[inline]
pub fn matches_complex<A: ElementAdapter>(
    adapter: &A,
    element: A::Handle,
    sel: &ComplexSelector,
) -> bool {
    let mut target = element;
    if sel.rest.is_empty() {
        return matches_compound(adapter, target, &sel.first);
    }

    if sel
        .rest
        .last()
        .is_some_and(|pair| !matches_compound(adapter, target, &pair.1))
    {
        return false;
    }

    // Walk remaining pairs from right-to-left, skipping the last which we've already matched.
    for pair in sel.rest.iter().rev().skip(1) {
        let combinator = pair.0;
        let right_compound = &pair.1;
        if let Some(next_target) =
            match_combinator_find(adapter, combinator, right_compound, target)
        {
            target = next_target;
        } else {
            return false;
        }
    }

    // Finally, relate the left-most compound (sel.first) via the first combinator.
    if let Some(first_pair) = sel.rest.first() {
        let first_combinator = first_pair.0;
        let first_right_comp = &first_pair.1;
        return match_combinator(
            adapter,
            first_combinator,
            first_right_comp,
            &sel.first,
            target,
        );
    }
    false
}

/// Match a compound selector against a single element.
/// Spec: Section 5–8
#[inline]
pub fn matches_compound<A: ElementAdapter>(
    adapter: &A,
    element: A::Handle,
    compound: &CompoundSelector,
) -> bool {
    for simple in &compound.simples {
        let owned = simple.clone();
        match owned {
            SimpleSelector::Universal => {}
            SimpleSelector::Type(type_name) => {
                if !type_name.is_empty() && adapter.tag_name(element) != type_name.as_str() {
                    return false;
                }
            }
            SimpleSelector::Class(class_name) => {
                if !adapter.has_class(element, class_name.as_str()) {
                    return false;
                }
            }
            SimpleSelector::IdSelector(id_value) => {
                if adapter
                    .element_id(element)
                    .is_none_or(|value| value != id_value.as_str())
                {
                    return false;
                }
            }
            SimpleSelector::AttrEquals { name, value } => {
                if adapter
                    .attr(element, name.as_str())
                    .is_none_or(|attr_value| attr_value != value.as_str())
                {
                    return false;
                }
            }
        }
    }
    true
}

/// Helper: Evaluate a combinator between two compounds, looking for a match and returning the matched left element.
/// Spec: Section 11 — Combinators
fn match_combinator_find<A: ElementAdapter>(
    adapter: &A,
    comb: Combinator,
    left_comp: &CompoundSelector,
    right_element: A::Handle,
) -> Option<A::Handle> {
    match comb {
        Combinator::Descendant => {
            let mut current_parent = adapter.parent(right_element);
            while let Some(ancestor_element) = current_parent {
                if matches_compound(adapter, ancestor_element, left_comp) {
                    return Some(ancestor_element);
                }
                current_parent = adapter.parent(ancestor_element);
            }
            None
        }
        Combinator::Child => {
            if let Some(parent_el) = adapter.parent(right_element)
                && matches_compound(adapter, parent_el, left_comp)
            {
                return Some(parent_el);
            }
            None
        }
        Combinator::AdjacentSibling => {
            if let Some(prev_el) = adapter.previous_sibling_element(right_element)
                && matches_compound(adapter, prev_el, left_comp)
            {
                return Some(prev_el);
            }
            None
        }
        Combinator::GeneralSibling => {
            let mut current_sibling = adapter.previous_sibling_element(right_element);
            while let Some(sibling_element) = current_sibling {
                if matches_compound(adapter, sibling_element, left_comp) {
                    return Some(sibling_element);
                }
                current_sibling = adapter.previous_sibling_element(sibling_element);
            }
            None
        }
    }
}

/// Helper: Evaluate a combinator between the left-most (sel.first) and the immediate right compound and element.
/// Spec: Section 11 — Combinators
fn match_combinator<A: ElementAdapter>(
    adapter: &A,
    comb: Combinator,
    _right_comp: &CompoundSelector,
    left_most: &CompoundSelector,
    right_element: A::Handle,
) -> bool {
    match comb {
        Combinator::Descendant => {
            let mut current_parent = adapter.parent(right_element);
            while let Some(ancestor_element) = current_parent {
                if matches_compound(adapter, ancestor_element, left_most) {
                    return true;
                }
                current_parent = adapter.parent(ancestor_element);
            }
            false
        }
        Combinator::Child => {
            if let Some(parent_el) = adapter.parent(right_element) {
                return matches_compound(adapter, parent_el, left_most);
            }
            false
        }
        Combinator::AdjacentSibling => {
            if let Some(prev_el) = adapter.previous_sibling_element(right_element) {
                return matches_compound(adapter, prev_el, left_most);
            }
            false
        }
        Combinator::GeneralSibling => {
            let mut current_sibling = adapter.previous_sibling_element(right_element);
            while let Some(sibling_element) = current_sibling {
                if matches_compound(adapter, sibling_element, left_most) {
                    return true;
                }
                current_sibling = adapter.previous_sibling_element(sibling_element);
            }
            false
        }
    }
}

/// A simple per-element, per-selector cache.
/// Users should call `invalidate_for_element` on attribute/class/id changes.
/// Spec: Section 3 — Matching is stable unless DOM or attributes change
#[derive(Default)]
pub struct MatchCache {
    /// Per-element per-selector memoized match results.
    store: HashMap<(u64, u64), bool>,
}

impl MatchCache {
    /// Cache a result.
    /// Spec: Section 3 — Caching is an optimization and not mandated by spec.
    #[inline]
    pub fn set(&mut self, element_key: u64, selector_key: u64, matched: bool) {
        self.store.insert((element_key, selector_key), matched);
    }

    /// Get a cached result.
    #[inline]
    pub fn get(&self, element_key: u64, selector_key: u64) -> Option<bool> {
        self.store.get(&(element_key, selector_key)).copied()
    }

    /// Invalidate cached results for an element (e.g., when class/attr/id changes).
    #[inline]
    pub fn invalidate_for_element(&mut self, element_key: u64) {
        self.store
            .retain(|&(element_key2, _), _| element_key2 != element_key);
    }
}

/// Build a stable key for a selector to use with `MatchCache`.
/// In production you might hash the serialized form. Here we use a simple hasher.
#[inline]
pub fn calc_selector_key(sel: &ComplexSelector) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Stable hash of the structure
    for simple in &sel.first.simples {
        simple.hash(&mut hasher);
    }
    for pair in &sel.rest {
        let combinator = pair.0;
        let comp = &pair.1;
        match combinator {
            Combinator::Descendant => 0u8.hash(&mut hasher),
            Combinator::Child => 1u8.hash(&mut hasher),
            Combinator::AdjacentSibling => 2u8.hash(&mut hasher),
            Combinator::GeneralSibling => 3u8.hash(&mut hasher),
        }
        for simple2 in &comp.simples {
            simple2.hash(&mut hasher);
        }
    }
    hasher.finish()
}
