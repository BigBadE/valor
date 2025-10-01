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
/// A simple selector consisting of a tag name, an element id, a list of classes, or the universal selector.
///
/// Universal selector reference: Selectors Level 4 — 2.2. Universal selector ('*')
/// <https://www.w3.org/TR/selectors-4/#universal-selector>
pub(crate) struct SimpleSelector {
    /// Optional tag name, lower-cased, for type selectors.
    tag: Option<String>,
    /// Optional element id, for `#id` selectors.
    element_id: Option<String>,
    /// Class list for `.class` selectors.
    classes: Vec<String>,
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
            universal: take(&mut current.universal),
        },
        combinator_to_next: Some(combinator),
    });
}

/// Compute the specificity (ids, classes, tags) for a parsed selector.
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
        if part.sel.tag.is_some() {
            tags = tags.saturating_add(1);
        }
    }
    Specificity(ids, classes, tags)
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
            if current.universal
                || current.tag.is_some()
                || current.element_id.is_some()
                || !current.classes.is_empty()
            {
                commit_current_part(&mut parts, &mut current, Combinator::Descendant);
                next_combinator = None;
            } else {
                next_combinator = Some(Combinator::Descendant);
            }
        }
        match chars.peek().copied() {
            None => break,
            Some('>') => {
                chars.next();
                if current.universal
                    || current.tag.is_some()
                    || current.element_id.is_some()
                    || !current.classes.is_empty()
                {
                    commit_current_part(&mut parts, &mut current, Combinator::Child);
                    next_combinator = None;
                } else {
                    next_combinator = Some(Combinator::Child);
                }
            }
            Some('*') => {
                // Universal selector — matches any element.
                // Selectors Level 4 §2.2: https://www.w3.org/TR/selectors-4/#universal-selector
                chars.next();
                current.universal = true;
            }
            Some('#') => {
                chars.next();
                current.element_id = Some(consume_ident(&mut chars, true));
            }
            Some('.') => {
                chars.next();
                current.classes.push(consume_ident(&mut chars, true));
            }
            Some(character) if character.is_alphanumeric() => {
                current.tag = Some(consume_ident(&mut chars, false));
            }
            Some(_) => {
                chars.next();
            }
        }
    }
    if current.universal
        || current.tag.is_some()
        || current.element_id.is_some()
        || !current.classes.is_empty()
    {
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
