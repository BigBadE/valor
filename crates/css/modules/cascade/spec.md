# CSS Cascading and Inheritance — Module Spec Checklist

Spec: https://www.w3.org/TR/css-cascade-4/

## Implemented (Speedometer MVP)
- [x] Section 3 — Origins and importance
  - [x] `Origin` integration (UA < User < Author)
  - [x] `important` flag handling
- [x] Section 5 — The cascade: sorting
  - [x] `CascadePriority { origin, important, specificity, source_order }`
  - [x] `rank_candidate()`
  - [x] `compare_priority()`
- [x] Section 6 — Specificity integration
  - [x] Uses `css_selectors::Specificity`
- [x] Section 7 — Inheritance (subset)
  - [x] `is_inherited_property()` for font-size, font-family, color
  - [x] `inherit_property()` fallback to parent or initial
- [x] Section 8 — Initial values (subset)
  - [x] `initial_value()` for font-size, font-family, color

## Parsing/Inputs
- [x] Candidates carry origin, importance, specificity, source order
- [x] Inputs originate from `css_syntax` rules filtered by `css_selectors`

## Algorithm overview
- [x] Rank declarations by `CascadePriority`
- [x] Pick winning declaration per property using `compare_priority()`
- [x] Apply inheritance fallback with `inherit_property()`

## Integration
- [x] Upstream: `css_selectors` for specificity
- [x] Downstream: computed-style model consumes resolved values
- [x] Orchestrator sequence: DOM changes → selectors → cascade → computed style

## Caching/Invalidation (guidance)
- [x] Recompute on class/id/attr changes, inline style changes, stylesheet updates, and parent computed-style changes

## Future work
- [ ] Cascade layers (`@layer`) ordering
- [ ] `revert`, `revert-layer`
- [ ] More inherited properties and property-defined inheritance rules
- [ ] Shorthand/longhand expansion tables
