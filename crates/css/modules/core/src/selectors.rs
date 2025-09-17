//! Selector parsing and specificity utilities for the core CSS engine.
use std::iter::Peekable;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Combinator {
    Descendant,
    Child,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SimpleSelector {
    tag: Option<String>,
    element_id: Option<String>,
    classes: Vec<String>,
}

impl SimpleSelector {
    #[inline]
    pub(crate) fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }
    #[inline]
    pub(crate) fn element_id(&self) -> Option<&str> {
        self.element_id.as_deref()
    }
    #[inline]
    pub(crate) fn classes(&self) -> &[String] {
        &self.classes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectorPart {
    sel: SimpleSelector,
    combinator_to_next: Option<Combinator>,
}

impl SelectorPart {
    #[inline]
    pub(crate) fn sel(&self) -> &SimpleSelector {
        &self.sel
    }
    #[inline]
    pub(crate) fn combinator_to_next(&self) -> Option<Combinator> {
        self.combinator_to_next.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selector(Vec<SelectorPart>);

impl Selector {
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
    #[inline]
    pub(crate) fn part(&self, index: usize) -> Option<&SelectorPart> {
        self.0.get(index)
    }
}

/// Specificity represented as (ids, classes, tags)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Specificity(pub u32, pub u32, pub u32);

#[inline]
/// Consume an identifier from a character iterator.
fn consume_ident<I>(chars: &mut Peekable<I>, allow_underscore: bool) -> String
where
    I: Iterator<Item = char>,
{
    let mut out = String::new();
    while let Some(&character) = chars.peek() {
        let ok = character.is_alphanumeric()
            || character == '-'
            || (allow_underscore && character == '_');
        if !ok {
            break;
        }
        out.push(character);
        chars.next();
    }
    out
}

#[inline]
fn commit_current_part(
    parts: &mut Vec<SelectorPart>,
    current: &mut SimpleSelector,
    combinator: Combinator,
) {
    parts.push(SelectorPart {
        sel: SimpleSelector {
            tag: current.tag.take(),
            element_id: current.element_id.take(),
            classes: std::mem::take(&mut current.classes),
        },
        combinator_to_next: Some(combinator),
    });
}

#[inline]
pub(crate) fn compute_specificity(selector: &Selector) -> Specificity {
    let mut ids = 0u32;
    let mut classes = 0u32;
    let mut tags = 0u32;
    for part in &selector.0 {
        if part.sel.element_id.is_some() {
            ids = ids.saturating_add(1);
        }
        if !part.sel.classes.is_empty() {
            classes = classes.saturating_add(part.sel.classes.len() as u32);
        }
        if part.sel.tag.is_some() {
            tags = tags.saturating_add(1);
        }
    }
    Specificity(ids, classes, tags)
}

#[inline]
fn parse_single_selector(selector_str: &str) -> Option<Selector> {
    let mut chars = selector_str.trim().chars().peekable();
    let mut parts: Vec<SelectorPart> = Vec::new();
    let mut current = SimpleSelector::default();
    let mut next_combinator: Option<Combinator> = None;
    let mut saw_whitespace = false;

    loop {
        // Consume whitespace as a descendant combinator boundary.
        while chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            saw_whitespace = true;
            chars.next();
        }
        if saw_whitespace {
            if current.tag.is_some() || current.element_id.is_some() || !current.classes.is_empty()
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
                if current.tag.is_some()
                    || current.element_id.is_some()
                    || !current.classes.is_empty()
                {
                    commit_current_part(&mut parts, &mut current, Combinator::Child);
                    next_combinator = None;
                } else {
                    next_combinator = Some(Combinator::Child);
                }
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
    if current.tag.is_some() || current.element_id.is_some() || !current.classes.is_empty() {
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

#[inline]
pub(crate) fn parse_selector_list(input: &str) -> Vec<Selector> {
    input.split(',').filter_map(parse_single_selector).collect()
}
