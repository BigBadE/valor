# DESIGN PLAN — BrowserBench Support Checklist

Last updated: 2025-09-12 22:49 local

Goal: Run browserbench.org suites (Speedometer 2/3, JetStream 2, MotionMark 1.3) to completion with valid scores. Prioritize JS/runtime/DOM/event/timing/Canvas2D surfaces. Defer non‑Latin text, full navigation/history, and accessibility.

Check off tasks as you finish them.

---

## Milestones and Checklists

### 1) Chrome UI: valor:// scheme and embedded resources
- [x] Add a `valor://` URL scheme that serves embedded chrome assets (HTML/CSS/JS)
- [x] Map `valor://chrome/*` to bytes via `include_bytes!` or `rust-embed`
- [x] Treat `valor://chrome` as a privileged origin with no network access

Crate mapping
- [x] page_handler::url::stream_url: support `valor://` and stream embedded resources
- [x] page_handler::state::HtmlPage: accept streams from embedded sources

Acceptance
- [x] `valor://chrome/index.html` loads as a normal page and produces a display list

---

### 2) Chrome UI: dual pages and compositor layers
- [x] Host two HtmlPage instances: `chrome_page` (UI) and `content_page` (site)
- [x] Extend renderer with simple compositor: multiple retained display lists in z‑order
- [x] Draw order: Background → Content → Chrome (alpha blended)

Crate mapping
- [x] wgpu_renderer::state::RenderState: `clear_layers()`, `push_layer(Layer::Chrome/Content)`, multi‑DL `render()`
- [x] valor (app): create both pages, update both each tick, install their display lists as layers

Acceptance
- [x] Both pages render in one frame; Chrome overlays Content correctly

---

### 3) Chrome UI: input routing and focus
- [x] Initial routing: fixed top‑bar band (e.g., 56 px) goes to `chrome_page`; otherwise to `content_page`
- [x] Add `HtmlPage::dispatch_pointer_event` and `dispatch_keyboard_event` to enqueue DOM events
- [x] Maintain focused page and basic cursor updates from chrome when applicable
- [ ] Future: optional display‑list hit‑testing to replace band routing

Crate mapping
- [x] valor (event loop): route `winit::WindowEvent` → chrome or content
- [x] page_handler::state::HtmlPage: event injection helpers (pointer/keyboard)
- [ ] layouter/renderer (optional): expose simple hit rectangles for future routing

Acceptance
- [x] Clicking the chrome address bar receives keyboard focus; content does not consume those keystrokes

---

### 4) Chrome UI: privileged bindings (`chromeHost`)
- [x] Provide a privileged host object to `valor://chrome` only: `navigate(url)`, `back()`, `forward()`, `reload()`, `openTab(url?)`, `closeTab(id)`
- [x] Ensure strict origin check to prevent exposure on non‑chrome pages
- [x] Wire bindings to app/page actions via a channel or callback trait

Crate mapping
- [x] js::bindings: register `chromeHost` when origin is `valor://chrome`
- [x] valor (app): implement trait/handler that performs navigation on `content_page`

Acceptance
- [x] Submitting the address bar form calls `chromeHost.navigate` and loads the requested URL in `content_page`

---

### 5) Chrome UI: assets (HTML/CSS/JS)
- [x] Create minimal chrome: back/forward buttons, address bar, go/reload, simple layout/top bar height
- [x] Keep to currently supported HTML/CSS (block/inline/flex if available); avoid complex effects early
- [x] `app.js` hooks form submit/clicks and calls `chromeHost` APIs
- [x] Embed assets and make them available under `valor://chrome/`

Crate mapping
- [x] assets/chrome (source) → embedded via include_bytes! into page_handler

Acceptance
- [x] Chrome UI is visible and interactive; current URL appears in the address bar

---

### 6) Chrome UI: state synchronization
- [ ] Reflect navigation changes from `content_page` to the chrome address bar
- [ ] Basic loading indicator (e.g., `document.readyState` or start/finish hooks)
- [ ] Back/forward buttons enable/disable according to history state

Crate mapping
- [ ] valor/page_handler: surface current URL and history availability to chrome via `chromeHost` events or a read API

Acceptance
- [ ] Navigating updates the address bar and button states without manual refresh

---

### 7) Chrome UI: testing, diagnostics, and fallback
- [ ] Unit tests for `valor://` loader and origin policy
- [ ] Integration test: chrome page loads and builds a non‑empty display list
- [ ] Manual smoke: address bar navigation to a few sites; input routing sanity
- [ ] Fallback: load `valor://chrome/fallback.html` if main chrome fails

Crate mapping
- [ ] valor/tests and page_handler/tests: coverage for loader and chrome bootstrap

Acceptance
- [ ] Tests pass; app remains usable if chrome assets are missing

---

### 8) Canvas 2D and requestAnimationFrame (MotionMark)
- [ ] `HTMLCanvasElement` and `CanvasRenderingContext2D` subset
  - [ ] State: `save/restore`, transforms, `globalAlpha`, `globalCompositeOperation` (source-over)
  - [ ] Paths: `beginPath/moveTo/lineTo/rect/arc/closePath`, `stroke/fill`, `lineWidth`, `lineCap`, `lineJoin`
  - [ ] Drawing: `fillRect`, `strokeRect`, `clearRect`
  - [ ] Text: `font`, `textAlign`, `fillText`, `measureText` (approx)
  - [ ] Images: `drawImage` from in-memory bitmap; `createImageBitmap` stub
- [ ] `requestAnimationFrame(callback)`; schedule before paint per frame; pass DOMHighResTimeStamp
- [ ] Backing implementation: software raster OK; no on-screen required for correctness

Crate mapping
- [ ] New `canvas` module or crate; optional reuse of wgpu_renderer display list
- [ ] page_handler::state::HtmlPage: rAF scheduling in frame loop

Acceptance
- [ ] MotionMark 1.3 basic suites start and complete

---

### 9) Benchmark harness and automation
- [ ] `bench_runner` binary to load local copies of BrowserBench suites and collect scores
- [ ] Tiny static file server or relaxed `file://` policy for local runs
- [ ] CLI flags for suite and iterations; JSON output
- [ ] CI job to run Speedometer and report scores

Crate mapping
- [ ] New crate or add to `valor` bin; reuse HtmlPage; console log parsing for score

Acceptance
- [ ] Speedometer runs headless and outputs stable score JSON

---

### 10) Fidelity, stability, performance
- [ ] Event loop ordering: `microtasks → rAF → rendering → timers` cadence
- [ ] Timer clamping for nested timers; optional background throttling
- [ ] Module loader caching and cycle handling
- [ ] Reduce overhead in DOMUpdate broadcasting for JS‑originated mutations (bypass or direct modes)
- [ ] Basic HTTP cache and same‑origin cookie jar to stabilize Speedometer

Crate mapping
- [ ] page_handler::state::HtmlPage: central event loop cadence (microtasks → rAF → rendering → timers)
- [ ] js: nested timer clamping; optional background throttling and page visibility hooks
- [ ] js/page_handler: module loader cache and cycle detection
- [ ] page_handler/js: HTTP cache and same-origin cookie jar for fetch/XHR

Acceptance
- [ ] Variance under 3–5% across runs; no hangs/unhandled rejections

---

## Concrete API Surface Checklist

Essentials
- [x] `window`, `document`, `console`
- [x] `setTimeout/clearTimeout`, `setInterval/clearInterval`
- [x] `queueMicrotask`
- [x] `performance.now`
- [ ] `requestAnimationFrame`

DOM Core
- [ ] Node: `nodeType`, `parentNode`, `childNodes`, `appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `cloneNode(deep)`, `textContent`
- [ ] Element: `id`, `className`, `classList`, `dataset`, `getAttribute/setAttribute/removeAttribute`, `hasAttribute`, `innerHTML/outerHTML`, `insertAdjacentHTML`, `matches`, `closest`
- [ ] Document: `createElement`, `createTextNode`, `createDocumentFragment`, `getElementById`, `querySelector(All)`

Events
- [x] `EventTarget` plumbing
- [x] `Event`, `MouseEvent`, `KeyboardEvent`, `CustomEvent`
- [ ] `InputEvent`

Networking
- [x] `fetch`, `Request`, `Response`, `Headers`
- [x] `XMLHttpRequest`

Data
- [ ] `URL`, `URLSearchParams`, `Blob`, `FormData`
- [ ] `TextEncoder`, `TextDecoder`

Storage
- [x] `localStorage`, `sessionStorage`

Observers
- [ ] `MutationObserver` (childList/attributes/characterData)

Graphics
- [ ] `HTMLCanvasElement`, `CanvasRenderingContext2D`, `ImageBitmap` (stub)
- [ ] `requestAnimationFrame`

Modules and scripts
- [x] Classic scripts ordering
- [ ] Module scripts and dynamic `import()`

---

## Test Strategy
- [ ] Unit tests per API area (timers, DOM ops, events, fetch/xhr, storage, mutation observer)
- [ ] Integration fixtures: minimal TodoMVC variant in tests/fixtures/bench/
- [ ] Optional curated WPT subsets (timers/microtasks, DOM Core, events, fetch basic, MutationObserver, Canvas 2D)
- [ ] Benchmark harness to download/mirror BrowserBench assets and record scores

---

## Risks and Mitigations
- [ ] Spec ordering subtleties → cover with focused unit tests + WPT subset
- [ ] Module loader complexity → start with same-origin static graph; defer import maps
- [ ] Canvas 2D performance → software path first; optimize hot paths later or leverage wgpu
- [ ] DOM perf under heavy churn → allow style/layout-disabled runs for benchmarks; reduce DOMUpdate overhead

---

## 6–8 Week Execution Plan (suggested)

Week 1–2
- [ ] Milestone 1: window/document, timers
- [ ] Milestone 2: DOM core, innerHTML, query APIs
- [ ] Tests for DOM and timers; simple Todo app boots

Week 3
- [ ] Milestone 3: Events
- [ ] Milestone 5: performance.now, storage
- [ ] Run TodoMVC variants; fix missing DOM bits

Week 4
- [ ] Milestone 4: Fetch/XHR
- [ ] Milestone 6: Classic scripts
- [ ] Load Speedometer 2.1 assets locally; first end-to-end run

Week 5
- [ ] Milestone 7: MutationObserver
- [ ] Stabilize Speedometer; add module scripts (Speedometer 3)

Week 6
- [ ] Milestone 8: Canvas 2D + rAF
- [ ] Run MotionMark basic suites

Week 7–8
- [ ] Milestone 9: Harness/CI automation
- [ ] Milestone 10: Fidelity/perf tuning
- [ ] Attempt JetStream 2 subset

---

## Acceptance Criteria (Definition of Done)
- [ ] Speedometer 2.1: Completes with numeric score > 0 on at least 2 runs, < 10% variance
- [ ] MotionMark 1.3: “Design” or “Multiply” tests complete; score stable within 15%
- [ ] JetStream 2: Harness loads, pure-JS tests run; overall score produced (document skip list)
- [ ] Automated `bench_runner` produces JSON reports locally/CI

---

## Architecture Mapping (reference)
- [ ] html::dom / parser: streaming updates; fragment parsing for innerHTML
- [ ] css / style_engine / layouter: unchanged for benchmarks except selector reuse for `matches/querySelector`
- [ ] js_engine_v8 / js: host bindings for DOM, timers, events, fetch/xhr, storage, canvas2d, modules
- [ ] page_handler: loader, origin context, event loop and frame cadence, script/module fetching
- [ ] wgpu_renderer (optional for MotionMark): may be bypassed by software canvas path


