# Module Spec Format (Production-Level)

This template defines the required structure and standards for each CSS/HTML module’s `spec.md`. It mirrors the layouter’s spec format and adds explicit coding/documentation standards for one-to-one spec mapping and implementation maturity (MVP/approximations vs production). In addition, it standardizes how the verbatim normative specification text may be embedded (see §11).

## 0. Location and naming

- Each module MUST provide a `spec.md` at the module root.
  - Examples:
    - `crates/css/modules/<module>/spec.md`
    - `crates/html/<module>/spec.md`
- The module-local `spec.md` is the single source of truth for spec coverage and implementation notes.
- One spec per file: each `spec.md` MUST correspond to exactly one primary specification (single TR URL).
  - Do not combine multiple specifications in one `spec.md`.
  - If a module implements pieces from multiple specifications, create a separate `spec.md` per spec (for example under submodules), and cross-link between them.

## 1. Title and primary spec(s)

- Top lines MUST include a module title and primary spec link(s):
  - `# <Module Name> — Spec Coverage Map (Spec version)`
  - `Primary spec: https://www.w3.org/TR/<SpecVersion>/` (exactly one primary link per `spec.md`)
  - Add additional spec links as needed (e.g., CSS2, CSS Display, CSS Sizing).

## 2. Scope and maturity

- Clearly describe the scope and current maturity:
  - State whether the module is MVP/prototype or production-level.
  - Explicitly list any approximations, subsets, heuristics, fallbacks, non-normative behaviors, or TODOs.
  - Use these tags inline wherever applicable:
    - `[MVP]`, `[Approximation]`, `[Heuristic]`, `[Fallback]`, `[Non-normative]`, `[TODO]`, `[Production]`.

## 3. Integrated verbatim spec with per-section status and mapping (REQUIRED)

- Each module `spec.md` MUST embed the relevant normative specification text (excluding non-spec front matter like Abstract, Status, and general Introductions) in a dedicated section.
- Verbatim spec text MUST be generated via the vendor script: `scripts/vendor_display_spec.ps1` (or the corresponding `.sh`) by providing the source TR URL and the module's `spec.md` path. After generation, you MUST merge your one-to-one mapping blocks into the generated document.
  - Precede the embedded text with the W3C legal notice (see §11 for template). Update `$name_of_software`, `$distribution_URI`, and `$year-of-software`.
  - The embedded spec MUST be organized in the same order as the original and MUST include every section relevant to the module.
  - Integration requirements:
    - Place a status/mapping block immediately after each corresponding spec heading (H2 or H3) and before the embedded text.
    - This block MUST include:
      - `Status:` one of `[MVP]`, `[Production]`, `[Approximation]`, `[Heuristic]`, `[Fallback]`, `[Non-normative]`, `[TODO]` (choose all that apply).
      - `Code:` exact symbols and short paths implementing the section (use backticks). Use crate/module-symbol notation consistent with code and avoid fully qualified Rust paths.
      - `Fixtures:` concrete test fixtures (full relative paths) or `<em>None</em>`.
      - `Notes:` brief deviations/approximations/heuristics/fallbacks as needed.
    - Wrap the actual normative spec text for each section in a `<details class="valor-spec">` block with a `<summary>Show spec text</summary>`.
      - H2 blocks MUST exclude their H3 subsections from the H2 `<details>` region.
      - Each H3 subsection gets its own `<details>` block.
  - Keep the auto-generated verbatim region markers intact (e.g., `<!-- BEGIN VERBATIM SPEC: DO NOT EDIT BELOW -->`). Insert or update your mapping blocks around headings without altering the vendor markers, so the vendor script can re-run safely.
  - This integrated mapping supersedes the old top-level checklist. You MAY keep a short “One-to-one spec mapping” section at the top to explain that mapping is integrated and to list any cross-spec mappings (e.g., to CSS 2.2) that don’t live in the primary TR.

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
  - Every non-trivial code block MUST include an inline comment that cites the exact clause it implements, and files SHOULD implement clauses in the same order as the spec.
    - Example: `// Spec: §8.1 Block Formatting Context — anonymous block generation` directly above the relevant block.
- File and module structure:
  - Mirror spec chapters where practical (`vertical.rs`, `horizontal.rs`, `height.rs`, etc.). See §12 for folder structure rules that mirror chapter/section numbering.
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
  - Hardcoded values policy: sections may NOT be marked `[Production]` if behavior depends on incorrectly hardcoded constants or placeholders. Such values MUST be properly wired from inputs/configuration and validated by fixtures before `[Production]` status is allowed.

## 12. Spec-driven folder structure and naming (enforced)

- Folder hierarchy MUST mirror the specification’s chapter/section layout, to make navigation and mapping trivial.
  - Create a folder per top-level chapter, named by the chapter number, followed by an underscore and a short lowercase slug (no dashes).
    - Examples: `8_block_formatting_context/`, `2_box_layout_modes/`.
  - For subsections, create nested folders using the dotted number prefix, optionally followed by a short kebab-case title.
    - Examples: `8/8.1/`, `8/8.1-anonymous-blocks/`, `2/2.3-list-items/`.
  - Place Rust source files that implement a clause inside the folder that matches that clause number.
    - Example: the implementation of “Chapter 8” lives under `.../<module>/8_block_formatting_context/`, and “§2.5 Box generation” may be implemented in `.../<module>/2_box_layout_modes/part_2_5_box_generation.rs`.
- The root `spec.md` in the module references exactly one primary spec (see §0 and §1) and its mapping section points to files inside these chapter/section folders.
- Comments and doc links in those files MUST cite the exact clause (e.g., `§8.1`, anchor id), and code should be ordered to follow the spec’s order whenever practical.

## 10. Future work

- Track planned steps to reach production conformance, using the same tags and explicit code pointers.

---

## 11. Verbatim spec embedding (integrated)

- Embed the relevant normative text directly in `spec.md` using the vendor script, and integrate status/mapping blocks in-line with headings as described in §3.
  - Exclude non-spec front matter such as Abstract, Status, and general Introduction sections.
  - Keep chapters/sections in spec order and clearly mark the beginning of the embedded verbatim block.
  - Include the following W3C legal notice ahead of the embedded text, replacing placeholders as indicated (keep the license URL intact):

```
$name_of_software: $distribution_URI
Copyright  [$year-of-software] World Wide Web Consortium. All Rights Reserved. This work is distributed under the W3C Software and Document License [1] in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
[1] https://www.w3.org/Consortium/Legal/copyright-software
```

 Begin embedded normative text below (exclude Abstract, Status, general Introduction). Keep chapters in spec order, and clearly indicate the source spec version and URL.

### Vendor workflow

- Use PowerShell (Windows) or Bash (Unix) vendor scripts:
  - PowerShell: `./scripts/vendor_display_spec.ps1 -SpecUrl "https://www.w3.org/TR/<SpecVersion>/" -ModuleSpecPath "crates/css/modules/<module>/spec.md" -Year "<year>"`
  - Bash: `./scripts/vendor_display_spec.sh "https://www.w3.org/TR/<SpecVersion>/" "crates/css/modules/<module>/spec.md" "<year>"`
- After the script generates/refreshes the verbatim block, insert or update the per-section status/mapping blocks immediately after the relevant H2/H3 headings, before the `<details class="valor-spec">` for that section.
- Do not edit inside the auto-generated verbatim region. Keep vendor markers intact so the script can be re-run idempotently.

For each section heading (H2/H3), insert immediately after the heading a status/mapping block and then wrap the spec text for that section in a details block. Example:

```html
<h3 id="list-items">2.3. …</h3>
<div data-valor-status="list-items">
  <p><strong>Status:</strong> [MVP]</p>
  <p><strong>Code:</strong> <code>css_display::chapter2::part_2_3_list_items::maybe_list_item_child</code></p>
  <p><strong>Fixtures:</strong> <em>None</em></p>
  <p><strong>Notes:</strong> …</p>
</div>
<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>
  <!-- verbatim spec HTML for §2.3 here -->
</details>
```

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

This information is integrated into the verbatim spec below via per-section blocks. Use this section only for cross-spec mappings that don’t belong to the primary TR.

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

## Verbatim Spec (integrated)

Legal notice (required when embedding spec text):

```
$name_of_software: $distribution_URI
Copyright  [$year-of-software] World Wide Web Consortium. All Rights Reserved. This work is distributed under the W3C Software and Document License [1] in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
[1] https://www.w3.org/Consortium/Legal/copyright-software
```

 Begin embedded normative text below (exclude Abstract, Status, general Introduction). Keep chapters in spec order, and clearly indicate the source spec version and URL.

### Vendor workflow

- Use PowerShell (Windows) or Bash (Unix) vendor scripts:
  - PowerShell: `./scripts/vendor_display_spec.ps1 -SpecUrl "https://www.w3.org/TR/<SpecVersion>/" -ModuleSpecPath "crates/css/modules/<module>/spec.md" -Year "<year>"`
  - Bash: `./scripts/vendor_display_spec.sh "https://www.w3.org/TR/<SpecVersion>/" "crates/css/modules/<module>/spec.md" "<year>"`
- After the script generates/refreshes the verbatim block, insert or update the per-section status/mapping blocks immediately after the relevant H2/H3 headings, before the `<details class="valor-spec">` for that section.
- Do not edit inside the auto-generated verbatim region. Keep vendor markers intact so the script can be re-run idempotently.

For each section heading (H2/H3), insert immediately after the heading a status/mapping block and then wrap the spec text for that section in a details block. Example:

```html
<h3 id="list-items">2.3. …</h3>
<div data-valor-status="list-items">
  <p><strong>Status:</strong> [MVP]</p>
  <p><strong>Code:</strong> <code>css_display::chapter2::part_2_3_list_items::maybe_list_item_child</code></p>
  <p><strong>Fixtures:</strong> <em>None</em></p>
  <p><strong>Notes:</strong> …</p>
</div>
<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>
  <!-- verbatim spec HTML for §2.3 here -->
</details>
```
```
