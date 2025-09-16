# CSS Style Attributes — Module Spec Checklist

Spec: https://www.w3.org/TR/css-style-attr/

## Scope
- MVP: Parse `style="..."` attribute into a sequence of simple declarations.
  - Split on `;`, split items on first `:`, trim ASCII whitespace, property lowercased ASCII.
  - Skip invalid/empty items.
  - Provide helpers to return a Vec of declarations and a HashMap with last-one-wins.
- Out of scope (for now):
  - Full tokenization per CSS Syntax.
  - `!important` parsing/precedence.
  - Value parsing into typed representations.
  - Error recovery beyond skipping invalid entries.
  - Integration with cascade specificity and the full style system.

## Spec Link(s)
- CSS Style Attributes Level 1 — Interpreting the style attribute: https://www.w3.org/TR/css-style-attr/#interpreting
- CSS Syntax for tokenization and whitespace definitions: https://www.w3.org/TR/css-syntax-3/

## Checklist (one-to-one mapping)
- [x] Interpreting style attributes — basic declaration splitting
  - [x] `parse_style_attribute()` — `crates/css/modules/style_attr/src/lib.rs`
  - [x] `parse_style_attribute_into_map()` — `crates/css/modules/style_attr/src/lib.rs`
  - [x] `normalize_style_attribute()` — helper to dedupe last occurrence per property
- [ ] Interpreting `!important` — value-level parsing and precedence mapping
  - [ ] Future parser layering with CSS Syntax tokens and an AST for declarations

## Parsing overview
- Minimal, resilient parser for early integration:
  - Split on semicolons, first colon per item.
  - Trim ASCII whitespace per Syntax definitions; lowercase ASCII property names.
  - Skip invalid entries. No tokenization, no `!important`.
- This is intentionally small and isolated so a proper tokenizer-backed implementation can replace it without changing public function signatures.

## Algorithms/Matching overview
- Not applicable beyond linear scan and last-one-wins when collapsed into a map.

## Caching/Optimization
- None currently. Helpers provide normalized/last-one-wins collapse for convenience.

## Integration
- Intended consumers:
  - `css` orchestrator or DOM mirror phases that ingest attributes to feed into cascade.
  - Future integration point: cascade module to merge style attribute declarations with stylesheets using proper origin and source order.
- Current status:
  - Self-contained utility with no cross-crate dependencies.

## Future work
- [ ] Implement tokenizer-backed declaration parsing using `syntax` module utilities.
- [ ] Support `!important` and tie into cascade precedence.
- [ ] Map parsed declarations into typed property values via modules (color, sizing, etc.).
- [ ] Wire into the style engine/orchestrator to affect computed styles.
