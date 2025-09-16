# CSS Custom Properties (Variables) — Module Spec Checklist

Spec: https://www.w3.org/TR/css-variables-1/

## Scope
- MVP resolver for `var()` usage within property values:
  - String-based resolution of `var(--name)` and `var(--name, fallback)`.
  - Environment lookup: prefer inherited (parent) custom properties, then current.
  - Recursive expansion and basic cycle detection.
  - Best-effort handling of unbalanced parentheses (leave literal).
- Out of scope (now):
  - Full tokenization with CSS Syntax tokens and value AST integration.
  - Typed property value resolution and cascade integration beyond string substitution.
  - Serialization and computed value time substitution rules.

## Spec Link(s)
- Using variables: https://www.w3.org/TR/css-variables-1/#using-variables
- Custom properties definition: https://www.w3.org/TR/css-variables-1/#custom-properties
- Cycles: https://www.w3.org/TR/css-variables-1/#cycles
- CSS Syntax (parentheses/whitespace): https://www.w3.org/TR/css-syntax-3/

## Checklist (one-to-one mapping)
- [x] Using variables — resolve `var()` against environments
  - [x] `resolve_vars_in_value()` — `crates/css/modules/variables/src/lib.rs`
  - [x] Cycle detection via resolution stack
  - [x] Fallback handling `var(--x, fallback)`
- [x] Custom properties environment extraction
  - [x] `extract_custom_properties()` — filter `--*` into a `HashMap`
- [ ] Integration with cascade and computed values
  - [ ] Replace string substitution with tokenizer/AST-based expansion
  - [ ] Property-by-property application at computed-value time

## Parsing overview
- Current implementation uses string scanning:
  - Detect `var(`, find matching `)`, split args on first comma.
  - Trim ASCII whitespace around arguments.
  - Recursive evaluation for nested `var()` inside referenced values and fallbacks.
- To be replaced later with CSS Syntax tokenization for correctness and edge cases.

## Algorithms/Matching overview
- Linear scan with recursion and a small stack to detect cycles as per spec guidance.

## Caching/Optimization
- None yet; intended to be fast enough for MVP. Future: cache expanded values by input and environment hash.

## Integration
- Intended consumers:
  - Cascade/computed value resolution phase to substitute `var()` calls using current + inherited custom properties.
- Current status:
  - Utility-only; does not yet hook into cascade. Signature chosen to ease future integration.

## Future work
- [ ] Switch to tokenizer-backed parsing from `css_syntax`.
- [ ] Integrate into cascade with proper inheritance and invalid-at-computed-value-time behavior.
- [ ] Implement spec-compliant error handling (invalid at computed value time) and propagation.
- [ ] Add unit tests covering nesting, fallback chains, and cycles.
