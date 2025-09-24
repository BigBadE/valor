# Module Spec Format (Production-Level)

This template defines the required structure and standards for each CSS/HTML module’s `spec.md`. It mirrors the layouter’s spec format and adds explicit coding/documentation standards for one-to-one spec mapping and implementation maturity (MVP/approximations vs production). In addition, it standardizes how the verbatim normative specification text may be embedded (see §11).

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

## 3. Verbatim spec (REQUIRED) with per-section status

- Each module `spec.md` MUST embed the complete relevant normative specification text (excluding non-spec front matter like Abstract, Status, and general Introductions) in a dedicated section.
- Precede the embedded text with the W3C legal notice (see §11 for template). Update `$name_of_software`, `$distribution_URI`, and `$year-of-software`.
- The embedded spec MUST be organized in the same order as the original and MUST include every section relevant to the module.
- At the start of each embedded section, add a status line in brackets indicating implementation maturity for that section, e.g.: `[Status: Production]`, `[Status: MVP]`, `[Status: Approximation]`.
- Immediately following each embedded section, add a concise mapping block with:
  - `Code:` exact symbols and file paths implementing the section.
  - `Notes:` deviations/approximations/heuristics/fallbacks.
  - `Fixtures:` concrete test fixtures (full relative paths).
- This replaces the old checklist; the verbatim text is now the source of truth and is annotated with status and mappings.

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
  - Keep code files under ~500 lines. Split large modules.
  - The ~500-line limit applies to source code files only; it does NOT apply to documentation like `spec.md`.
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

## 11. Verbatim spec embedding (optional but encouraged)

- You MAY embed the entire relevant normative text of the specification directly into the module’s `spec.md`, to facilitate one-to-one mapping and cross-referencing.
  - Exclude non-spec front matter such as Abstract, Status, and general Introduction sections.
  - Keep chapters/sections in spec order and clearly mark the beginning of the verbatim appendix.
  - Include the following W3C legal notice ahead of the embedded text, replacing placeholders as indicated (keep the license URL intact):

```
$name_of_software: $distribution_URI
Copyright © [$year-of-software] World Wide Web Consortium. All Rights Reserved. This work is distributed under the W3C® Software and Document License [1] in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
[1] https://www.w3.org/Consortium/Legal/copyright-software
```

- Keep the embedded verbatim text in a dedicated appendix section (see boilerplate below). The full text can exceed 500 lines; the code-file line limit does not apply to documentation.

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

---

## Verbatim Spec Appendix (optional)

Legal notice (required if embedding spec text):

```
$name_of_software: $distribution_URI
Copyright © [$year-of-software] World Wide Web Consortium. All Rights Reserved. This work is distributed under the W3C® Software and Document License [1] in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
[1] https://www.w3.org/Consortium/Legal/copyright-software
```

Begin embedded normative text below (exclude Abstract, Status, general Introduction). Keep chapters in spec order, and clearly indicate the source spec version and URL.
```
