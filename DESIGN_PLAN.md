# Valor Engine: Production Roadmap and Phased Checklist

This document lays out a practical, phased plan to evolve Valor from a solid prototype into a production‑grade browser engine. It converts previously identified gaps into an actionable checklist with clear phases, deliverables, exit criteria, and risk notes.

The plan is designed around Valor’s existing architecture:
- html (DOM, parser), css (parser/types), layouter (layout tree mirror), wgpu_renderer (render backend), page_handler (orchestration), style_engine (computed styles), valor (app entry/event loop).
- Streaming DOMUpdate → DOM → DOMMirror<T> pipeline with sibling mirrors (Layouter, planned StyleEngine).

Quick links to key actors/types invoked in this plan:
- page_handler::state::HtmlPage, html::dom::{DOM, updating::{DOMUpdate, DOMMirror, DOMSubscriber}}, html::parser::{HTMLParser, html5ever_engine::Html5everEngine/ValorSink}
- layouter::Layouter (+ layout::* modules), style_engine::{ComputedStyle, Display, SizeSpecified, Edges}
- css::parser::StylesheetStreamParser, css::types::{Stylesheet, StyleRule, Declaration, Origin}
- wgpu_renderer::state::RenderState

Conventions:
- [ ] = not started, [*] = in progress, [x] = complete
- Each phase lists Exit Criteria. Phases can overlap where dependencies allow.


## Phase 0 — Stabilization & Groundwork
Focus: harden the current prototype, improve test coverage and developer ergonomics to unblock larger changes.

Deliverables
- [x] Formal API docs for layouter, style_engine, and DOM mirror types (rustdoc pass). 
- [x] Add a minimal "docs/" site with architecture overview and this plan (rendered in GitHub Pages optional).
- [x] Expand unit tests around the refactored layouter (block/inline partition, margin collapsing, percent sizing, auto sizing).
- [x] Add simple benchmarks for layout pass (criterion) to establish a baseline.
- [x] Introduce feature flags to gate big subsystems (style_engine_full, shaping, retained_display_list, compositor).

Exit Criteria
- Clean build on stable toolchain without warnings on main platforms.
- CI: unit tests + layout geometry tests run in < 2 minutes.
- Baseline perf numbers recorded for layout micro-benchmarks.


## Phase 1 — Style Engine and Invalidation
Focus: a first real CSS cascade, variable resolution, and targeted invalidation, without chasing every CSS feature.

1. Parsing and model
- [x] Expand css::parser to normalize shorthands → longhands for: margin, padding, border (width/color/style as longhands), font, background (subset).
- [x] Implement CSS variable token capture (var() references) without resolution.
- [x] Support author vs user vs user-agent origin and source order.

2. Cascade and computed style
- [x] Implement selector matching across type, id, class, attribute, descendant/child combinators (no complex pseudo-classes yet).
- [x] Specificity + importance ordering; inheritance for standard inherited properties.
- [x] Variable resolution (var() + fallback), with cycle detection and initial values.
- [x] Box model resolution: display, position, width/height/min/max, margin/padding/border.
- [x] Text properties: font family/size/weight/style, line-height (number/length).
- [x] Provide ComputedStyle sharing cache (structural hash + arena) to reduce allocations.

3. Invalidation
- [x] Build a selector dependency graph keyed by id/class/attr/tag changes.
- [x] On DOMUpdate batches: map changes → affected style nodes, mark dirty bits (STYLE) on layouter mirror.
- [x] Coalesce and schedule style recompute before layout.

4. Integration
- [x] StyleEngine as a DOMSubscriber; maintain a map NodeKey → ComputedStyle.
- [x] Expose a read-only handle to Layouter (layouter.computed_styles()).
- [x] Tests: selector correctness, cascade order, inheritance, var() behaviors.

Exit Criteria
- Deterministic style computation for a suite of HTML+CSS fixtures (sanity vs Chromium screenshots or reference geometry).
- Style invalidation updates only restyle affected subtrees on attribute/class/id changes.

Risks/Notes
- Selector engine performance must be acceptable on 100–1,000 nodes; optimize later if needed.


## Phase 2 — Layout Tree and Fragmentation Scaffold
Focus: separate DOM from a proper layout (box) tree, and introduce a fragment tree to represent lines and breaks.

- [x] Introduce a LayoutBox tree (block/inline/anonymous, containing formatting context data).
- [x] Build from DOM + ComputedStyle: DOM → Style (existing map) → LayoutBox tree builder.
- [x] Introduce a Fragment tree to represent generated boxes (line boxes, fragments across breaks).
- [x] Migrate layouter algorithms to operate on LayoutBox/Fragment rather than DOM directly.
- [x] Keep DOMMirror for incremental updates; map NodeKey → LayoutBoxId.
- [x] Tests: fragment creation for inline runs, anonymous block generation rules.

Exit Criteria
- Existing simple pages render via LayoutBox/Fragment path with parity to current geometry tests.

Risks/Notes
- This is mostly structural; aim for minimal behavior changes.


## Phase 3 — Formatting Contexts (Flex + Positioned + Overflow/Scroll basics)
Focus: broaden beyond basic block/inline.

Flexbox
- [x] Support display:flex and display:inline-flex (row, nowrap initially).
- [x] Implement flex sizing: flex-basis, flex-grow/shrink, min/max constraints.
- [x] Cross-axis alignment: align-items (baseline/center/start/end subset).

Positioning
- [x] Absolute positioning with containing block resolution.
- [x] Fixed positioning relative to viewport; establish stacking context flags.
- [x] Sticky positioning (subset; threshold-based initial version).

Overflow/Scroll
- [x] overflow:auto/scroll clipping; create scroll containers.
- [x] Basic scroll state in layout tree and expose scrollable regions to renderer.

Exit Criteria
- Flex test pages layout correctly (intrinsic sizes, wrapping off), absolute/fixed examples position correctly, and overflow clipping works.

Risks/Notes
- Flex correctness can be subtle; prioritize spec-aligned tests.


## Phase 4 — Text Shaping and Internationalization
Focus: accuracy of text layout.

- [x] Integrate HarfBuzz for shaping; create text run building from computed font + script.
- [x] Font system: font fallback chain, font cache (per face+size+features), font loading (@font-face minimal).
- [x] Line breaking using UAX #14 rules; hyphenation hooks; whitespace processing per CSS Text 3.
- [x] Bidirectional (Unicode Bidi) reordering for inline content; isolate formatting characters.
- [x] Measure text using glyph metrics; replace char_width approximations.
- [x] Basic whitespace collapsing (white-space: normal approximation) in layout and rendering.

Exit Criteria
- Internationalized sample pages render with correct shaping and line breaks (Latin, Arabic, CJK), visual parity on core cases.

Risks/Notes
- Font loading and shaping can introduce async complexity; cache aggressively


## Phase 5 — Incremental & Parallel Layout
Focus: robust invalidation, partial reflow, and scheduling.

- [x] Expand DirtyKind into distinct style/layout/paint dirty bits; track reasons and axes (inline/block).
- [x] Maintain a queue of dirty roots; topologically schedule partial reflow.
- [x] Cache ancestor constraints (available inline/block size) to isolate layout to affected subtrees.
- [x] Coalesce DOM/style updates per frame; integrate with a FrameScheduler (16.6ms budget target).
- [x] Explore parallel layout of independent subtrees (read-only shared state; careful with fonts/style cache).
- [x] Benchmarks to measure partial vs full reflow wins.

Exit Criteria
- Typical DOM mutations (class toggle, text edit) invalidate and reflow only minimal regions; perf improves vs baseline.


## Phase 6 — Painting, Retained Display List, and Compositing
Focus: retained rendering pipeline and GPU compositing.

Display List
- [ ] Introduce a retained display list (DL) structure with items (rect, text, image, clip, transform, opacity, border, background).
- [ ] Build DL from Fragment tree; compute clips/stacking contexts.
- [ ] Diffing: compute minimal DL updates from dirty regions.

Compositing
- [ ] Layerization heuristics (positioned, transform, opacity, video/canvas) → compositor layers.
- [ ] Implement a simple compositor in wgpu_renderer with render passes per layer and proper z-ordering.
- [ ] Add picture caching/tiling for large content (basic version).

Visual Fidelity
- [ ] Implement border radii, box-shadow, text selection highlight, backgrounds (color/gradient subset).

Exit Criteria
- Smooth scrolling and animations on modest pages with partial DL rebuilds and GPU compositing; correctness on stacking/clip basics.

Risks/Notes
- Pay attention to precision and pixel snapping at different scales/DPI.


## Phase 7 — Interactivity and Accessibility
Focus: hit-testing, input, selection/caret, and an accessibility tree.

- [ ] Hit testing API mapping screen → fragment → DOM node; expose event regions.
- [ ] Text selection geometry; caret placement; keyboard navigation.
- [ ] Focus management; :focus styles; tabindex traversal.
- [ ] Accessibility (AX) tree derived from layout: roles, names, states; platform bridges (skeletons).

Exit Criteria
- Pointer events and text selection behave correctly on test pages; AX tree generated for sample forms/documents.

Risks/Notes
- AX semantics depend on DOM + style; keep it incremental.


## Phase 8 — Performance, Memory, and Telemetry
Focus: operability and production diagnostics.

- [ ] Memory: arenas/pools for layout and fragment allocations; DL/item arenas.
- [ ] Frame scheduler: FPS budget adherence; rAF cadence; throttling under load.
- [ ] Profiling hooks and tracing spans around parse/style/layout/paint.
- [ ] Production telemetry (behind a feature flag); crash/ICE reporting harness.

Exit Criteria
- Stable FPS on common pages; sustained memory bounded with no pathological growth; actionable profiles.

Risks/Notes
- Avoid premature micro-optimizations; let profiles guide work.


## Phase 9 — Conformance and Quality
Focus: test coverage, fuzzing, and CI gates.

- [ ] Integrate a subset of web-platform-tests (style/layout/paint tiers); establish pass rates per tier.
- [ ] Seed corpus-based fuzzers for html/css parsers and layout/style glue.
- [ ] Golden image tests (DL snapshots) for painter/compositor.
- [ ] CI: per-PR run of fast tiers; nightly broader sweeps.

Exit Criteria
- Regressions caught by CI gates; steady increase in WPT passes over time.


## Cross-Cutting Checklists

APIs and Docs
- [*] Rustdoc completeness for public types and functions across crates.
- [ ] Examples for subscribing a new mirror (e.g., StyleEngine) and building a simple page pipeline.

DevEx
- [ ] Repro scripts for layout/paint perf tests.
- [ ] Script to generate minimal DL snapshots for visual testing.

Security/Privacy (initial)
- [ ] Sandboxing assumptions documented for future JS engine integration.
- [ ] Same-origin and subresource loading notes (HTML/CSS fetch paths) documented.


## Milestones and Suggested Sequencing
- Milestone A (P0–P1): Style Engine v1 + invalidation. Target: 4–6 weeks.
- Milestone B (P2): Layout/Fragment trees. Target: 3–4 weeks.
- Milestone C (P3): Flexbox + basic positioning/overflow. Target: 4–6 weeks.
- Milestone D (P4): Text shaping/I18N v1. Target: 4–8 weeks.
- Milestone E (P5–P6): Incremental layout + retained DL/compositor v1. Target: 6–10 weeks.
- Milestone F (P7–P9): Interactivity, perf/telemetry, conformance. Ongoing.


## Adoption Plan
- Convert each checkbox to tracked issues with labels: phase:<N>, area:(style|layout|paint|i18n|perf|ax|infra), type:(feature|bug|test|doc).
- Gate larger subsystems behind feature flags; land incrementally.
- Maintain a small always-on geometry test suite to guard regressions during refactors.


---
This roadmap reflects the architecture and data flow already present in Valor and provides a step-by-step path toward production readiness while allowing iterative delivery and validation at each stage.