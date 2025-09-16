# Module Spec Documentation Format

This document standardizes how each CSS module documents its mapping to the W3C specification.

## Location and naming
- Each module must include a `spec.md` file at the module root.
  - Example: `crates/css/modules/values_units/spec.md`, `crates/css/modules/selectors/spec.md`.
- Project-wide docs can reference these module-local spec files as the single source of truth.

## Required sections in `spec.md`
1. Scope
   - Define the current implementation scope (e.g., MVP subset for Speedometer).
   - Clearly list out-of-scope items with examples for later work.
2. Spec Link(s)
   - Link to the relevant W3C specification(s), including section anchors when helpful.
3. Checklist (one-to-one mapping)
   - Use a checklist style to track implementation status.
   - For each spec chapter/section, add `[x]` for implemented or `[ ]` for planned.
   - Map each item to code symbols (types, functions, modules). Use backticks for symbol names and include file paths where helpful.
4. Parsing overview (if applicable)
   - Summarize tokenization and parsing strategy; call out deviations from spec.
5. Algorithms/Matching overview (if applicable)
   - Summarize evaluation strategy (e.g., right-to-left selector matching), key invariants, and performance notes.
6. Caching/Optimization (optional)
   - Document any non-normative caches or indexes and invalidation strategy.
7. Integration
   - Explain how other modules integrate with this one (e.g., cascade or layout entry points).
8. Future work
   - Track planned extensions beyond the current scope.

## Code documentation requirements
- Every public function/type must include a short doc comment that references the spec section it implements.
  - Use the format: `/// Spec: <https://www.w3.org/TR/<spec>#<section>>`.
  - Include a concise one-line description first, then spec reference lines.
- Prefer chapter-structured source files or nested modules to mirror the spec table of contents where practical.
- Keep doc comments concise and relevant. Avoid inline spec blocks; link out to the spec instead.

## Example
See `crates/css/modules/values_units/spec.md` and `crates/css/modules/values_units/src/lib.rs` for a good baseline:
- Module-level documentation block at the top of `lib.rs`.
- Per-function doc comments that reference specific spec sections.
- Module-local `spec.md` that provides a chapter-by-chapter mapping and scope.

## Conventions
- Use backticks for code symbols (e.g., `parse_number()`), and include full relative paths when helpful.
- Keep `spec.md` focused on normative mapping and implementation notes. Avoid duplicating tutorial content.
- If a previous doc lived under `docs/<module>/...`, replace it with a short pointer to the module-local `spec.md`.

## Template snippet
Use the following pattern when creating a new module spec:

```markdown
# <Module Name> — Module Spec Checklist

Spec: <https://www.w3.org/TR/...>

## Implemented (MVP)
- [x] Section/Chapter N — Title
  - [x] `SymbolOrFunction()`
  - [ ] Planned sub-item

## Parsing/Inputs or Algorithms (as applicable)
- [x] Short checklist items here

## Integration
- [x] Upstream dependency
- [x] Downstream consumer

## Future work
- [ ] Planned feature 1
- [ ] Planned feature 2
```
