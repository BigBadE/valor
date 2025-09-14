# Valor Architecture Overview

This document summarizes Valor’s current data flow and key actors, aligned with the current codebase after the HtmlPage refactor phases (4–10).

High-level Flow
1) App startup
   - valor::main creates the window and Tokio runtime.
   - `page_handler::state::HtmlPage` is constructed for a given URL using a `ValorConfig`.
2) Page wiring
   - `HtmlPage` sets up two channels for DOM updates:
     • out_updater: broadcast::Sender<Vec<DOMUpdate>> — DOM broadcasts applied updates.
     • in_updater: mpsc::Sender<Vec<DOMUpdate>> — producers send updates into the DOM.
   - `html::dom::DOM` is created with these channels and owns the runtime tree.
   - `HTMLParser` runs on a task, streaming bytes and producing batched DOMUpdate values.
3) DOM updates and mirroring
   - DOM drains its inbound mpsc receiver, applies each `DOMUpdate`, and re-broadcasts the batch via `out_updater`.
   - Components subscribe via `DOMMirror<T>` (`T: DOMSubscriber`). Examples:
     • `Layouter` — mirrors DOM to a simplified layout tree, computes geometry and snapshots.
     • `StyleEngine` — mirrors DOM and computes `ComputedStyle` per node.
     • `Renderer` — mirrors DOM to a scene graph representation.
     • `DomIndex` — supports `getElementById` and fast lookups.
4) CSS discovery and parsing (initial wiring)
   - `HTMLParser` discovers `<link rel="stylesheet">` and `<style>`.
   - `css::parser::StylesheetStreamParser` parses chunks into `css::types::Stylesheet` with origin + source order.
   - `StyleEngine` receives `Stylesheet`(s), indexes rules, and computes a `ComputedStyle` map.
5) Layout and rendering
   - `Layouter` reads `ComputedStyle` to extract layout-relevant properties and computes per-node geometry (`LayoutRect`).
   - A `DisplayBuilder` adapter assembles display lists from `RetainedInputs` (rects, text, overlays) — `HtmlPage` delegates to this trait.
   - `wgpu_renderer` consumes display lists to draw.

6) JavaScript runtime and events
   - The JS engine (`js_engine_v8::V8Engine`) hosts a runtime prelude that shims `window`/`document`, timers, and events.
   - A small `JsRuntime` trait (in `page_handler`) coordinates the per-tick sequence:
     • `tick_timers_once(page)` and `drive_after_dom_update(page)` (pending scripts + DOMContentLoaded) — default implementation delegates to `HtmlPage` internals.
   - Event dispatch supports document-level capture/target/bubble phases; full node-level `EventTarget` is planned.

7) ES modules
   - A `ModuleResolver` trait (in `js`) with a `SimpleFileModuleResolver` implementation bundles side-effect-only modules by stripping `import`/`export` and concatenating dependencies.
   - `HtmlPage` evaluates bundled code via `eval_module`.

Key Types
- js::DOMUpdate: InsertElement, InsertText, SetAttr, RemoveNode, EndOfDocument.
- js::DOMMirror<T>, js::DOMSubscriber: mirror pattern for propagating DOM mutations.
- page_handler::state::HtmlPage: orchestrates channels, parser, DOM, mirrors, and delegates to runtime and display builders.
- html::dom::DOM: owns runtime DOM and channels.
- style_engine::StyleEngine: computes NodeKey → ComputedStyle (UA defaults + author rules), subscribes to DOM.
- layouter::Layouter: mirrors DOM, tracks dirtiness, computes geometry; exposes dirty rects for renderer.
- page_handler::display::{DisplayBuilder, DefaultDisplayBuilder, RetainedInputs}.
- page_handler::runtime::{JsRuntime, DefaultJsRuntime}.
- js::modules::{ModuleResolver, SimpleFileModuleResolver}.
- page_handler::accessibility::ax_tree_snapshot_from.
- page_handler::state::UpdateOutcome — structured result of an update tick.

Layout Notes (today)
- Basic block/inline formatting with approximated text metrics (char width, line-height multiplier).
- Inline flow groups inline text and inline elements into lines; block children are stacked with simple vertical margin collapsing.
- Percent width resolves against the container content width; auto width fills available content.
- Incremental groundwork exists (DirtyKind, cached geometry, dirty rects), with a fallback to full layout.

Style Notes (today)
- UA defaults for display, font size, margins, etc., are sketched in StyleEngine.
- Author stylesheet support is present; origin and source-order support is being expanded in Phase 1.
- ComputedStyle contains minimal properties used by the layouter.

Where it’s going
- Event system: implement node-level `EventTarget`s with capture/target/bubble, `preventDefault`, and propagation controls.
- Module resolver: MIME/type checks, HTTP(S) fetching, caching, and robust URL resolution.
- Accessibility: extract to its own crate with richer roles/names and testing.
- API: keep `HtmlPage` thin; continue consolidating read-only snapshots and structured outcomes.

See also
- DESIGN_PLAN.md — phased roadmap and checklists.
- crates/layouter/src/layout/* — modular layout implementation details.
