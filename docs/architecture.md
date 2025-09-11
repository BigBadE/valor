# Valor Architecture Overview

This document summarizes Valor’s current data flow and key actors, adapted from the design notes and aligned with the current codebase.

High-level Flow
1) App startup
   - valor::main creates the window and Tokio runtime.
   - page_handler::state::HtmlPage is constructed for a given URL.
2) Page wiring
   - HtmlPage sets up two channels for DOM updates:
     • out_updater: broadcast::Sender<Vec<DOMUpdate>> — DOM broadcasts applied updates.
     • in_updater: mpsc::Sender<Vec<DOMUpdate>> — producers send updates into the DOM.
   - html::dom::DOM is created with these channels and owns the runtime tree.
   - HTMLParser runs on a task, streaming bytes and producing batched DOMUpdate values.
3) DOM updates and mirroring
   - DOM drains its inbound mpsc receiver, applies each DOMUpdate, and re-broadcasts the batch via out_updater.
   - Components subscribe via DOMMirror<T> (T: DOMSubscriber). Examples:
     • Layouter — mirrors DOM to a simplified layout tree, computes geometry.
     • StyleEngine — mirrors DOM and computes ComputedStyle per node.
4) CSS discovery and parsing (initial wiring)
   - HTML parser discovers <link rel="stylesheet"> and <style>.
   - css::parser::StylesheetStreamParser parses chunks into css::types::Stylesheet with origin + source order.
   - StyleEngine receives Stylesheet(s), indexes rules, and computes a ComputedStyle map.
5) Layout and rendering
   - Layouter reads ComputedStyle to extract layout-relevant properties (display, margin, padding, width/height).
   - Layout computes per-node geometry (LayoutRect) using a simple block/inline model today.
   - wgpu_renderer consumes geometry/display lists (future: retained list + compositor) to draw.

Key Types
- js::DOMUpdate: InsertElement, InsertText, SetAttr, RemoveNode, EndOfDocument.
- js::DOMMirror<T>, js::DOMSubscriber: mirror pattern for propagating DOM mutations.
- page_handler::state::HtmlPage: orchestrates channels, parser, DOM, mirrors.
- html::dom::DOM: owns runtime DOM and channels.
- style_engine::StyleEngine: computes NodeKey → ComputedStyle (UA defaults + author rules), subscribes to DOM.
- layouter::Layouter: mirrors DOM, tracks dirtiness, computes geometry; exposes dirty rects for renderer.

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
- Phase 1: a more complete cascade (selectors, specificity, inheritance, variables) and targeted invalidation.
- Phase 2: decouple into a LayoutBox tree and a Fragment tree.
- Phase 5–6: incremental/parallel layout and a retained display list with a compositor.

See also
- DESIGN_PLAN.md — phased roadmap and checklists.
- crates/layouter/src/layout/* — modular layout implementation details.
