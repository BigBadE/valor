Title: Incremental Layout Plan for Valor

Context
- Current behavior: Layouter::compute_layout() is called every update tick from HtmlPage::update(), recomputing the full tree even when nothing changed.
- Goal: Move to an event-driven, incremental layout system that computes only when needed and only for impacted subtrees, while preserving correctness and debuggability.

Objectives and Success Metrics
- Reduce unnecessary layout work by gating recomputation on actual DOM/style changes.
- Support subtree reflow driven by structural or style invalidation.
- Maintain determinism: same input updates produce same layout.
- Performance targets (initial):
  - 0 recomputes when no DOM/CSS/style changes occurred within a tick.
  - For localized mutations (e.g., text change in a leaf), reflow affects only the minimal ancestor chain and siblings impacted by inline/flow rules.
  - 2–5x speedup on synthetic benchmarks with small edits vs full recompute.
- Observability: counters for batches processed, nodes invalidated, subtrees reflowed, total time spent.

High-Level Phases
1) Groundwork: Dirty Scheduling and API Surfaces
2) Invalidation: DOM and Style changes → Layout dirtiness
3) Incremental Layout Algorithm (block/inline MVP)
4) Scheduling & Coalescing (frame boundary, debounce, throttle)
5) Renderer Integration (display list regeneration by region)
6) Testing, Benchmarks, and Telemetry
7) Roll-out, Flags, and Documentation

Phase 1 — Groundwork: Dirty Scheduling and APIs
Work Items
- Add a layout_dirty flag and last_change_epoch to Layouter.
- Have DOMMirror<T>::update() return whether updates were applied (or expose a has_pending/was_applied accessor). For now, Layouter can set its flag internally when apply_update is called.
- HtmlPage::update(): call compute_layout() only when layout_dirty or when style_changed.
- Expose Layouter::take_and_clear_layout_dirty() to atomically read and clear.
Acceptance Criteria
- Layout compute is skipped when there are no DOM/style changes.
- Existing tests continue to pass.

Phase 2 — Invalidation: DOM and Style → Layout Dirtiness
Work Items
- In Layouter.apply_update_impl():
  - Mark layout_dirty on structural changes (InsertElement, InsertText, RemoveNode).
  - Mark attribute/style dirtiness on SetAttr; if the attribute affects layout (placeholder heuristic for MVP: any attr change marks dirty).
- Add per-node DirtyKind flags: Structure, Style, Geometry.
- Track ancestor chain dirtying: when a node becomes dirty, mark ancestors up to root with at least Geometry.
Acceptance Criteria
- After a localized DOM update, only the affected nodes carry dirty flags; root is not marked unless necessary.

Phase 3 — Incremental Layout Algorithm (MVP)
Scope
- Current layout model is a simplified block/text flow. Implement subtree reflow for affected branches.
Work Items
- Refactor layout::compute_layout(self) into:
  - compute_layout_full(&self) for reference and tests.
  - compute_layout_incremental(&mut self, dirty_roots: &[NodeKey]).
- Maintain cached layout results per node (preferred: separate map NodeKey → LayoutRect and layout metadata: min/max, preferred sizes, line breaks info for text).
- Reflow algorithm:
  - For each dirty root, recompute layout for that subtree; bubble size changes to ancestors until geometry stabilizes or reaches a stable boundary.
  - For inline text changes, recompute line layout for the parent block only, then ancestors for geometry updates.
- Provide a policy to fallback to full layout when dirty set is large (e.g., > 30% of nodes).
Acceptance Criteria
- Unit tests show subtree-only work for localized mutations.
- Full fallback remains correct.

Phase 4 — Scheduling & Coalescing
Work Items
- Coalesce multiple DOMUpdate batches per tick:
  - HtmlPage collects updates during a frame and triggers layout once per frame.
- Debounce micro-changes: allow batching for a short interval (e.g., 1–2 ms) in the async runtime.
- Add a per-frame Budget (optional): if compute exceeds budget, split across frames; renderer uses last stable state.
Acceptance Criteria
- Layout runs at most once per tick by default.
- Stress tests with bursty updates remain responsive.

Phase 5 — Renderer Integration
Work Items
- Track regions that changed (dirty rects) from reflow.
- Provide Renderer with a display list diff or dirty rect set to redraw.
- Optional: cache text glyph layout for unchanged runs.
Acceptance Criteria
- Renderer redraws only affected regions for localized changes (MVP can still redraw entire scene; diff infra optional but scoped).

Phase 6 — Testing, Benchmarks, Telemetry
Work Items
- Add unit tests in layouter for dirty propagation and incremental reflow.
- Extend tests/layouter_chromium_compare.rs to assert single recompute for no-op ticks.
- Add simple micro-benchmarks measuring nodes processed for:
  - Full layout
  - Single leaf text change
  - Attribute change on an inner block
- Instrumentation: counters (updates_applied, nodes_reflowed, dirty_subtrees, layout_time_ms).
Acceptance Criteria
- Tests pass; counters demonstrate expected reductions.

Detailed Task List (Backlog)
A. Layouter API and State
- [x] A1: Add layout_dirty: bool and last_change_epoch: u64 to Layouter.
- [x] A2: Add dirty_map: HashMap<NodeKey, DirtyKind> with bitflags: STRUCTURE, STYLE, GEOMETRY.
- [ ] A3: Add cached_layout: HashMap<NodeKey, LayoutRect>.
- [x] A4: Methods: mark_dirty(node, kind), mark_ancestors_dirty(node, kind), take_and_clear_layout_dirty().

B. HtmlPage Scheduling
- [x] B1: Gate compute_layout() calls behind should_layout predicate combining: layouter.take_and_clear_layout_dirty() OR style_changed.
- [ ] B2: Introduce layout_debounce_deadline (Instant) optional for coalescing.
- [x] B3: Ensure mirrors update order: DOM → CSS → StyleEngine → Layouter → compute (conditional) → Renderer.

C. Invalidation Sources
- [x] C1: DOM updates mapping to dirty kinds:
  - InsertElement/InsertText/RemoveNode → STRUCTURE | GEOMETRY.
  - SetAttr → STYLE for now; later, detect layout-affecting attrs.
  - EndOfDocument → none.
- [ ] C2: StyleEngine changes:
  - On style_changed, mark STYLE for nodes with changed computed styles.

D. Incremental Reflow Engine (MVP)
- [ ] D1: Enumerate dirty roots by collecting nodes with STRUCTURE or STYLE without dirty parents (root set minimization).
- [ ] D2: For each root, recompute subtree geometry; update cached_layout.
- [ ] D3: Bubble geometry deltas upward: if parent sizes change, mark parent GEOMETRY and continue.
- [ ] D4: Fallback: if dirty set > threshold, run compute_layout_full().

E. Renderer Diffing (Optional for MVP)
- [ ] E1: From cached_layout before/after, compute dirty rects.
- [ ] E2: Pass dirty rects to Renderer; keep full redraw as fallback.

F. Telemetry & Debugging
- [ ] F1: Add logs/counters for applied updates, dirty nodes, reflowed nodes, skipped computes.
- [ ] F2: Expose a debug dump endpoint: layouter.print_dirty_state().

G. Tests & Benchmarks
- [ ] G1: Unit tests for dirty propagation logic.
- [ ] G2: Integration test: mutation that doesn’t affect layout shouldn’t trigger compute when feature enabled.
- [ ] G3: Benchmarks for localized edits vs full layout.

Risks and Mitigations
- Risk: Complexity of inline layout and line breaking. Mitigation: MVP recomputes parent block fully for any text change.
- Risk: Feature gates and async scheduling introduce racey behavior. Mitigation: Keep compute on same thread/tick after updates; coalesce only within tick.
- Risk: Incorrect dirty-root minimization leading to missed reflow. Mitigation: Conservative approach—prefer to over-invalidate early.

Milestones
- M1: Dirty scheduling (A1, B1) and gating compute — 1–2 days. [Completed]
- M2: Dirty tracking per node and invalidation mapping (A2, C1, C2) — 2–3 days. [Partially Completed: A2, C1]
- M3: Incremental reflow MVP with fallback (A3, D1–D4) — 4–6 days.
- M4: Tests, telemetry, and benchmarks (F1–F2, G1–G3) — 2–3 days.
- M5: Optional renderer diffing (E1–E2) — 2–4 days.

Implementation Notes and Pointers
- HtmlPage: crates/page_handler/src/state.rs — location to gate compute.
- Layouter: crates/layouter/src/lib.rs — add dirty state, cached layout, and incremental APIs.
- StyleEngine: crates/style_engine — expose which nodes’ computed styles changed.
- DOMUpdate mapping: crates/html/dom/updating.rs — source of updates consumed by Layouter.

Definition of Done
- Incremental layout feature behind a flag; when enabled, layout runs only when required and limits work to affected subtrees.
- Measurable performance improvements on synthetic tests; correctness parity with full layout on integration tests.
