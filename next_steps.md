### Overall objective
Wire CSS into the pipeline so Layouter consumes computed styles for each element and produces geometry consistent with browser behavior (or a defined subset), supporting incremental updates via DOMUpdate and (future) CSS updates.

---

### Phase 0 — Define scope and MVP subset
- [x] Properties to support first: display, margin/padding/border (physical), width/height, box-sizing, background-color (optional, for debugging), color, font-size, line-height (number/normal), white-space (normal), text-align (left/center/right), font-weight (normal/bold), overflow (visible/hidden), position (static), vertical margins collapse behavior (block only, later).
- [x] Selector subset: type, id, class, descendant/child combinators; no pseudo-classes/pseudo-elements initially.
- [x] Origins and cascade order: user-agent < author < user; implement UA and author only initially; include inline style attribute as highest author priority.
- [x] Units: px only initially; later add percentages and em.
- [x] Layout modes: block formatting context for non-replaced blocks; inline text simplified (existing InlineText nodes) with line-height used for vertical metrics.
- Status: Phase 0 initialized and scope defined on 2025-09-08 01:56.

---


### Phase 2 — StyleEngine skeleton and data structures
1) Subscriber wiring
    - [x] Implement StyleEngine: DOMSubscriber that keeps a mirror of the DOM for selector matching and receives stylesheet updates.
    - [x] Store per-element computed style cache keyed by NodeKey.
2) Selector matching indexes
    - [x] Maintain indexes for efficient match: by id → nodes, by class → nodes, by tag → nodes; fall back to DOM traversal for combinators.
3) Rule set organization
    - [x] Flatten Stylesheet into an internal RuleDB with tuples: (origin, source_order, selector_ast, declarations, specificity).
    - [x] Precompute specificity once per selector.

Deliverables:
- [x] StyleEngine struct with: dom mirror, RuleDB, indexes, and a map<NodeKey, ComputedStyle> (initially empty/defaults).

Checklist:
- Simple selector matcher and indexes
  - [x] Implement type/id/class selectors with descendant/child combinators; maintain indexes and basic traversal.
  - [ ] Acceptance: Author rules from stylesheets match elements in fixtures.
---

### Phase 3 — Cascade and computed style model
1) Define style types
    - Create Value enums for each supported property (e.g., LengthPx, Auto, Keyword, ColorRGBA). Reuse css::types where convenient; define a ComputedStyle with strongly-typed fields.
2) Default/initial values
    - Provide initial values per CSS spec (or simplified) used when not specified and not inherited.
3) Inheritance
    - Mark properties as inherited (color, font-size, line-height, font-family later). Implement inheritance from parent’s computed style at cascade time.
4) Cascade algorithm
    - Gather all matching rules for an element, sort by (origin, important, specificity, source order). Apply declarations in order to produce specified values.
    - Apply inline style attribute as highest priority author origin.
    - Convert specified values to computed values (resolve keywords to concrete values where possible; em/percentage may remain deferred until units supported).
5) Shorthands (later)
    - For MVP, accept only longhands or expand shorthands in the parser before storing in RuleDB.

Deliverables:
- ComputedStyle struct and a function: compute(element, parent_style, matched_rules) → ComputedStyle.

Checklist:
- ComputedStyle expansion and cascade
  - Define specified/computed/used model for width/height/margins/padding/display.
  - Implement cascade that merges UA + author (including inline styles) with specificity and source order.
  - Acceptance: Inline and <style> rules apply with correct priority.
- Inheritance basics
  - Implement inheritance for color, font-size, line-height (unitless multiplier) at least.
  - Acceptance: Inline text line-height derives from computed value.

---

### Phase 4 — Invalidation and incremental updates
1) DOM-driven invalidation
    - On InsertElement/InsertText: mark new element and descendants dirty for compute.
    - On SetAttr (id/class/style): update indexes and mark element (and possibly descendants, for class changes) dirty.
    - On RemoveNode: drop computed styles for subtree and remove from indexes.
2) CSS-driven invalidation
    - When a stylesheet is added/changed, determine which selectors could be affected; MVP: mark all elements dirty.
    - Later optimization: selector dependency graph to narrow impact (e.g., .class, #id, tag names).
3) Dirty propagation rules
    - If inheritance-affecting properties change on a node, mark descendants dirty for recompute (because inherited values changed).
4) Batching
    - Integrate with DOMMirror<T>::update: recompute styles for dirty set after draining a batch; produce a “StyleUpdated” notification for Layouter.

Deliverables:
- Dirty set tracking and recompute pass triggered per DOM/CSS update batch.

Checklist:
- Invalidation model (MVP)
  - On DOM SetAttr(id/class/style), update selector indexes and mark node dirty; on stylesheet updates, mark all dirty.
  - Recompute dirty set at the end of each batch and publish computed snapshot for Layouter.
  - Acceptance: Changing inline style or adding a rule updates layout without restart.

---


### Phase 6 — Apply styles in layout algorithms
1) Display and tree building
    - Determine if node participates in layout (display: none → omit; block → Block box; inline → contribute to inline formatting; others later).
2) Box model
    - Incorporate margin/padding/border into block layout calculations.
    - Respect box-sizing: content-box vs border-box for width/height.
3) Sizing
    - Resolve width/height: auto vs fixed px; compute containing block width; apply min/max constraints (later).
4) Margins
    - Support vertical margin collapsing (MVP can skip, but tests should know). Horizontal margins for inline replaced later.
5) Text metrics
    - Use font-size and line-height to compute InlineText line boxes; a simplified font measurement for now (line-height approximated from property values).
6) Backgrounds/painting (optional initially)
    - Use background-color for debug draw list.

Deliverables:
- Layout results change when styles are present; block geometry reflects margins/padding/borders and specified sizes.

Checklist:
- Immediate correctness and integration
  - Align body/html margins with tests or computed styles.
    - Replace hardcoded body_margin with value read from computed styles on html/body, honoring the test CSS reset.
    - Acceptance: In Chromium comparison, y and height for body/root align within epsilon without ad-hoc offsets.
  - Vertical block flow and margin collapsing (MVP).
    - Collapse parent top margin with first child’s top margin; collapse adjacent sibling vertical margins.
    - Ensure box border-box height derives from content + padding (margins consume flow but are not in the box).
    - Acceptance: First block child y matches Chromium in box_model.html.
  - Inline formatting refinement (single-line MVP).
    - Remove arbitrary inter-item spacing; base widths solely on measured text approximation.
    - Produce a single line box using line-height; inline boxes participate horizontally.
    - Acceptance: inline_block.html inline elements’ widths and y align within epsilon.
- Used-value resolution consistency (border-box/content sizing)
  - Adopt a single path for used width/height, assuming box-sizing: border-box under test reset.
  - Resolve % widths against containing block content width; pass base explicitly to children.
  - Acceptance: Percent width children (50%) match Chromium under parent width.
- Remove layout-time CSS parsing fallbacks in Layouter
  - Eliminate ad-hoc parsing of inline styles in layout; rely exclusively on StyleEngine’s computed values.
  - Acceptance: Tests still pass with Layouter depending only on computed styles.

---

### Phase 7 — Units, percentages, and fonts
1) Units
    - Add percentage resolution for width/height/margins against containing block.
    - Add em/ex resolution based on parent’s computed font-size.
2) Fonts
    - Introduce a minimal FontProvider to get ascent/descent/advance widths; or stub with approximate metrics until text rendering arrives.

Deliverables:
- More realistic inline layout and percentage behaviors.

Checklist:
- Units and value model
  - Introduce Environment (viewport sizes, device pixel ratio) for used-value resolution.
  - Expand units: px, %, em, rem; resolve % against correct bases.
  - Add min/max-width/height clamps in used-value step.
  - Plan for calc()/min()/max()/clamp() parsing later.

---

### Phase 8 — Selector coverage and cascade completeness
1) Add support for attribute selectors, adjacent/sibling combinators.
2) Add :root, :first-child, :last-child (simple structural) — via DOM mirror data.
3) Specificity and !important
    - Implement !important handling; order within origin.

Deliverables:
- Broader CSS compatibility on common sites and tests.

---

### Phase 9 — Performance and memory
- Rule matching optimization: pre-index selectors by rightmost simple selector; short-circuit on tag/class/id mismatch.
- Cache match results per node per rule-set epoch, invalidated by relevant attr changes.
- Avoid full-tree recomputes by using dirty propagation and incremental layout invalidation.

---

### Phase 10 — Testing and validation
1) Unit tests
    - Selector matching, specificity, cascade order, inheritance.
    - ComputedStyle for representative scenarios (inline style, author sheet, UA sheet).
2) Integration tests
    - Feed DOMUpdate batches and assert ComputedStyle maps produced by StyleEngine.
    - Layouter geometry tests: margin/padding/width/height and display none.
3) Chromium comparison tests
    - Use existing test harness to compare rects for simple pages; inject a reset to align defaults, or explicitly include UA margin expectations in both engines.
4) Fuzz/snapshot tests
    - Generate random DOM + simple styles; assert invariants and stability.

---



### Notes to avoid common pitfalls
- Body default margin: either mirror Chromium’s UA sheet or apply a reset in tests to avoid phantom diffs.
- Inline style attribute: ensure parsed and applied with highest author priority.
- Display defaults: without a UA sheet, many elements won’t lay out as expected; set default display per tag.
- Invalidation: class/id changes must update indexes and recompute style; inheritance changes must dirty descendants.
- Ordering: maintain monotonic source order for all stylesheets to guarantee stable cascade.

This outline should let you stage styling support into the layouter with clear milestones, while keeping incremental updates and testability in mind.

---

