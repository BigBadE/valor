# CSS Display — Spec Coverage Map (CSS Display 3, CSS 2.2)

Primary spec(s):
- https://www.w3.org/TR/css-display-3/
- https://www.w3.org/TR/CSS22/

## Scope and maturity

- Status: [MVP]
- Non-production items:
  - [MVP] Anonymous block synthesis for inline runs.
  - [MVP] Inline formatting context (line boxes) and inline sizing from shaped text.
  - [TODO] Integration with block/inline formatting contexts across BFC boundaries.

## One-to-one spec mapping (sorted by spec order)

- CSS Display 3 — Box generation and display tree normalization
  - Status: [x] [MVP]
  - Spec: https://www.w3.org/TR/css-display-3/
  - Code:
    - `crates/css/modules/layouter/src/box_tree.rs::flatten_display_children()` — `display: none` skipping and `display: contents` lifting. [MVP]
  - Notes:
    - [TODO] Move additional display-normalization helpers here as they are implemented.
  - Fixtures:
    - `crates/css/modules/display/tests/fixtures/layout/inline/12_anonymous_blocks.html`

- CSS 2.2 §9.4.1 — Anonymous block boxes around inline content
  - Status: [ ] [MVP]
  - Spec: https://www.w3.org/TR/CSS22/visuren.html#anonymous-block-level
  - Code:
    - [Planned] `crates/css/modules/display/src/anonymous_blocks.rs` — synthesize anonymous blocks for contiguous inline runs.
  - Notes:
    - [MVP] Current layouter treats inline simplistically; anonymous blocks need to be generated here and surfaced to layouter.
  - Fixtures:
    - `crates/css/modules/display/tests/fixtures/layout/inline/12_anonymous_blocks.html`

- CSS 2.2 — Inline formatting context and line boxes
  - Status: [ ] [MVP]
  - Spec: https://www.w3.org/TR/CSS22/visuren.html#inline-formatting
  - Code:
    - [Planned] `crates/css/modules/display/src/inline_formatting.rs` — build line boxes, measure inline content.
  - Notes:
    - [MVP] Minimal shrink-to-fit via shaped text width can be staged before full line-box construction.
  - Fixtures:
    - `crates/css/modules/display/tests/fixtures/layout/inline/04_block_inline_partition.html`

## Algorithms and data flow

- Entry points (planned):
  - `display::normalize_tree()` — perform display tree normalization (display: none/content, anonymous blocks).
  - `display::build_inline_context()` — produce line boxes and inline fragments.
- Integration:
  - Upstream: `style_engine::ComputedStyle`.
  - Downstream: `layouter` consumes normalized tree and inline fragments for layout passes.

## Edge cases and conformance nuances

- `display: contents` and anonymous block interactions with margin collapsing and BFC boundaries.
- Whitespace collapsing within inline formatting context (to be implemented here, not in layouter).

## Testing and fixtures

- Fixtures under: `crates/css/modules/display/tests/fixtures/layout/inline/**`.
- To ignore a fixture (expected failure), rename it with the `.fail` extension. The harness only runs `.html` fixtures; `.fail` files are skipped by discovery.

## Documentation and coding standards

- All public functions must include spec links with exact section anchors.
- Modules mirror spec chapters (`anonymous_blocks.rs`, `inline_formatting.rs`, etc.).
- Files kept under ~500 lines; imports at the top; avoid fully qualified paths (use imports/aliases).
- Mark non-production behavior `[MVP]`, `[Approximation]`, or `[Heuristic]` and link back here.

## Future work

- [ ] Implement anonymous block synthesis and export to layouter.
- [ ] Implement inline formatting context with line boxes.
- [ ] Add whitespace collapsing and inline flow edge cases.
