//! CSS selector matching engine.
//! Spec: <https://www.w3.org/TR/selectors-3/>

use crate::{
    Combinator, ComplexSelector, CompoundSelector, ElementAdapter, SelectorList, SimpleSelector,
};

/// Match a selector list against an element.
/// Spec: Section 3, 4
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
