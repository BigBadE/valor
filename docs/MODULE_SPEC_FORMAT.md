# Module Spec Format (Production-Level)

This template defines the required structure and standards for each CSS/HTML module’s `spec.md`. It mirrors the layouter’s spec format and adds explicit coding/documentation standards for one-to-one spec mapping and implementation maturity (MVP/approximations vs production).

## 0. Location and naming

- Each module MUST provide a `spec.md` at the module root.
  - Examples:
    - `crates/css/modules/<module>/spec.md`
    - `crates/html/<module>/spec.md`
- The module-local `spec.md` is the single source of truth for spec coverage and implementation notes.

## 1. Title and primary spec(s)

- Top lines MUST include a module title and primary spec link(s):
  - `# <Module Name> — Spec Coverage Map (Spec version)`
  - `Primary spec: https://www.w3.org/TR/<SpecVersion>/`
  - Add additional spec links as needed (e.g., CSS2, CSS Display, CSS Sizing).

## 2. Scope and maturity

- Clearly describe the scope and current maturity:
  - State whether the module is MVP/prototype or production-level.
  - Explicitly list any approximations, subsets, heuristics, fallbacks, non-normative behaviors, or TODOs.
  - Use these tags inline wherever applicable:
    - `[MVP]`, `[Approximation]`, `[Heuristic]`, `[Fallback]`, `[Non-normative]`, `[TODO]`, `[Production]`.

## 3. One-to-one spec mapping (checklist)

- For each relevant spec chapter/section, include a checklist mapping to code symbols. For each item:
  - Mark status with `[x]` implemented or `[ ]` planned.
  - Provide a short description and rationale if partial.
  - Map to concrete code symbols with file paths, using backticks, e.g. ``lib.rs::function_name()``.
  - Reference the exact spec section with an anchor URL.
  - Include a `Fixtures` subsection listing the concrete test fixtures that cover this chapter/section (full relative paths under `crates/**/tests/fixtures/**`).
  - Chapters MUST be sorted in ascending spec order (e.g., 8.1, 8.3.1, 9.4.1, 9.4.3, 10.3.3, 10.6).

Example entry:

- 8.3.1 Collapsing margins — CSS 2.2
  - Status: `[Production]` or `[MVP]`
  - Spec: https://www.w3.org/TR/CSS22/box.html#collapsing-margins
  - Code:
    - `visual_formatting/vertical.rs::apply_leading_top_collapse()` — leading group computation.
    - `layouter/lib.rs::compute_collapsed_vertical_margin()` — first-child and sibling collapsing.
    - `layouter/lib.rs::collapse_margins_list()` — algebra of extremes.
  - Notes:
    - `[Approximation]` Minimal BFC detection; see TODO.
  - Fixtures:
    - `crates/css/modules/box/tests/fixtures/layout/basics/03_margin_collapsing.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_basic.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_border_top.html`

## 4. Algorithms and data flow

- Summarize the core algorithms and how data flows between modules:
  - Entry points and orchestrators.
  - Key helpers, invariants, and error conditions.
  - Performance considerations (caching, reuse, asymptotics) where relevant.

## 5. Parsing/inputs (if applicable)

- Describe input models (e.g., tokens, computed styles, DOM updates) and any deviations from the spec.

## 6. Integration points

- List upstream dependencies and downstream consumers, with explicit interfaces:
  - Example: `style_engine::ComputedStyle` inputs, `DisplayList` outputs.
  - Any cross-crate type aliases preferred over fully qualified paths.

## 7. Edge cases and conformance nuances

- Call out behaviors that are subtle or historically inconsistent across engines.
- Include links to web-platform-tests or fixture references, and how the module matches the spec.

## 8. Testing and fixtures

- Document test harness usage and fixture locations:
  - Auto-discovery patterns.
  - Expected-failure/ignored fixtures: use the `.fail` extension to mark a fixture as ignored by discovery; `.html` fixtures run.
  - Any synthetic asserts coming from browser-side probes.

## 9. Documentation and coding standards (enforced)

- One-to-one mapping discipline:
  - Every public function/type MUST include a concise doc comment with a spec reference line:
    - `/// Spec: <https://www.w3.org/TR/<spec>#<section>>`
  - Prefer short, clear summaries; use links instead of copying spec text.
- File and module structure:
  - Mirror spec chapters where practical (`vertical.rs`, `horizontal.rs`, `height.rs`, etc.).
  - Keep files under ~500 lines. Split large modules.
- Imports and style (Rust):
  - Add `use` imports at the top; avoid fully qualified paths in code.
  - If a name collision occurs, import with an alias.
  - Leave an empty line between top-level multi-line items; avoid deep nesting; keep functions <100 lines.
  - Do not bypass `must_use` with `let _`/`drop`.
  - Don’t add `#[allow(...)]` unless explicitly permitted (tests excluded).
- Maturity labels in code/comments:
  - Any non-production behaviors MUST be tagged inline with one of: `[MVP]`, `[Approximation]`, `[Heuristic]`, `[Fallback]`, `[Non-normative]`.
  - Link to the section in `spec.md` that justifies the deviation and tracks its TODO.

## 10. Future work

- Track planned steps to reach production conformance, using the same tags and explicit code pointers.

---

## Boilerplate template for `spec.md`

```markdown
# <Module Name> — Spec Coverage Map (<Spec Version>)

Primary spec: https://www.w3.org/TR/<SpecVersion>/

## Scope and maturity

- Status: [MVP|Production]
- Non-production items: [Approximation] …; [Heuristic] …; [Fallback] …
- Out of scope (for now): …

## One-to-one spec mapping

- <Chapter/Section> — Title
  - Status: [x] / [ ]  [MVP|Production]
  - Spec: <link>
  - Code:
    - `<path>::symbol()` — purpose
    - `<path>::symbol()` — purpose
  - Notes: [Approximation] … [TODO] …
  - Fixtures:
    - `crates/<crate>/tests/fixtures/<area>/<fixture>.html`

## Algorithms and data flow

- Entry points: …
- Helpers: …
- Invariants: …

## Parsing/inputs (if applicable)

- …

## Integration

- Upstream: …
- Downstream: …

## Edge cases

- …

## Testing and fixtures

- Fixtures: `crates/<crate>/tests/fixtures/<area>/`
- Harness flags: …
- Ignored fixtures: use the `.fail` extension (ignored by discovery); `.html` fixtures run

## Documentation and coding standards

- Doc comments include spec links; modules mirror spec chapters; imports at top; functions short and focused.

## Future work

- [ ] Item 1 (link to code + spec)
- [ ] Item 2 (link to code + spec)
```
