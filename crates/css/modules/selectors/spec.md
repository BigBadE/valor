# Selectors Level 3 — Module Spec Checklist

Spec: https://www.w3.org/TR/selectors-3/

## Implemented (Speedometer MVP)
- [x] Chapter 3 — Overview
  - [x] `ElementAdapter` abstraction
  - [x] `matches_selector_list()` and `matches_complex()` (right-to-left)
  - [x] `MatchCache` memoization (non-normative)
- [x] Chapter 4 — Groups of selectors
  - [x] `SelectorList`
  - [x] `parse_selector_list()` splitting on commas
- [x] Chapter 5 — Type and universal selectors
  - [x] `SimpleSelector::Type`, `SimpleSelector::Universal`
  - [x] Tokenizer support and tag matching
- [x] Chapter 6 — Class selectors
  - [x] `SimpleSelector::Class`, tokenizer `.class`, adapter `has_class()`
- [x] Chapter 7 — ID selectors
  - [x] `SimpleSelector::Id`, tokenizer `#id`, adapter `id()`
- [x] Chapter 8 — Attribute selectors (subset)
  - [x] `SimpleSelector::AttrEquals { name, value }`
  - [x] Tokenizer `[attr=value]` with/without quotes
- [x] Chapter 11 — Combinators
  - [x] Descendant, Child `>`, Adjacent `+`, General sibling `~`
  - [x] `match_combinator_find()` / `match_combinator()` helpers
- [x] Chapter 13 — Specificity
  - [x] `Specificity` and calculators for compound/complex selectors

## Parsing and matching overview
- [x] Minimal tokenizer for MVP
- [x] Whitespace → descendant combinator

## Caching (non-normative)
- [x] `MatchCache` with selector hashing via `calc_selector_key()`
- [x] `invalidate_for_element()` guidance

## Integration
- [x] Used by cascade to determine applicable rules and specificity

## Future work
- [ ] Pseudo-classes (:not, :is, :where, etc.) and pseudo-elements
- [ ] Extended attribute operators and case-insensitivity
- [ ] Namespaces; language-sensitive matches
- [ ] Performance: compilation/indexing and invalidation strategies
