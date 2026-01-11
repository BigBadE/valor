# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Valor is a browser engine written in Rust, implementing HTML parsing, CSS styling, layout computation, and rendering with GPU acceleration.

DO NOT USE CARGO CLEAN WITHOUT EXPLICIT PERMISSION. REBUILDS TAKE FOREVER.

## Essential Commands

### Building and Testing
- **Run all code standards checks**: `./scripts/verify.sh`
  - This runs: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all --all-features`
  - **IMPORTANT**: Always run this before finishing work on any changes
- **Build the project**: `cargo build`
- **Run the browser**: `cargo run`
- **Run fixture tests**: `cargo test --test chromium_tests`
  - **CRITICAL**: Do NOT use `--all-features` with fixture tests - this enables the `js` feature and causes V8 isolate errors
  - Fixture tests are designed to run WITHOUT JavaScript
  - Tests compare Valor's layout and graphics output against Chromium
- **Run tests for specific package**: `cargo test --package valor`

### JavaScript Engine Features
The project supports JavaScript execution via an optional feature flag:
- **Default (no JS)**: `cargo build` - No JavaScript engine included
- **With JS feature**: `cargo build --features js` - Enables V8 JavaScript engine via `page_handler/js` feature
- The `js` crate (DOM types, mirrors) is always available, but `js_engine_v8` is optional

## Architecture

### Crate Structure

The workspace is organized into focused crates:

**Core browser crates:**
- **`valor`**: Main application entry point with windowing and event loop
- **`page_handler`**: Central coordinator managing page state, orchestrating HTML, CSS, JS, and rendering
- **`html`**: HTML5 parsing (html5ever) and DOM tree management
- **`css`**: CSS parsing and style computation orchestration
- **`renderer`**: Backend-agnostic scene graph and display list generation
- **`wgpu_backend`**: WGPU-based GPU rendering implementation
- **`js`**: JavaScript engine abstraction layer and DOM bindings
- **`js_engine_v8`**: V8 JavaScript engine implementation (with optional stub)

**CSS modules (in `crates/css/modules/`):**

The CSS subsystem follows a spec-driven development model. Each module contains a `spec.md` file that embeds the relevant W3C specification text with inline status annotations (e.g., `[Production]`, `[MVP]`, `[TODO]`, `[Approximation]`) and code location mappings.

*Active modules (uncommented in workspace):*
- **`orchestrator`**: Coordinates style computation across CSS modules
- **`core`**: Central style and layout engine (`StyleComputer` and `LayoutEngine`); embeds CSS 2.2 box model and visual formatting model spec
- **`syntax`**: CSS tokenization and parsing primitives
- **`selectors`**: Selector matching (Level 3); right-to-left matcher with caching
- **`style_attr`**: Inline `style="..."` attribute parser
- **`variables`**: CSS custom properties (`var()` resolution with cycle detection)
- **`color`**: Color value parsing (named, hex, rgb/rgba)
- **`display`**: Display tree normalization, anonymous blocks, inline formatting context (CSS Display 3)
- **`box`**: Box model (margin/padding/border, box-sizing)
- **`flexbox`**: Flexbox layout (Level 1); single/multi-line with gaps, baseline alignment
- **`text`**: Whitespace collapsing, default line-height/font-size utilities (Level 3)
- **`position`**: Positioned layout (absolute/fixed positioning; sticky deferred)
- **`cascade`**: Cascading and inheritance (origin, specificity, source order)
- **`values_units`**: Value parsing (numbers, percentages, lengths: px/em/rem/vw/vh)

*Inactive modules (commented out; planned/partial):*
- `backgrounds_borders`, `conditional_rules`, `fonts`, `images`, `media_queries`, `namespaces`, `sizing`, `text_decoration`, `transforms`, `ui`, `writing_modes`

Many modules are commented out in workspace dependencies, indicating progressive implementation.

### Data Flow

1. **HTML Parsing → DOM**: `html` crate parses HTML5 streams and builds a DOM tree using `indextree::Arena`
2. **DOM Updates**: DOM emits `DOMUpdate` events via broadcast channels to all subscribers
3. **Style Computation**: `css` crate's `Orchestrator` receives DOM updates and computes styles using `css_orchestrator::CoreEngine`
4. **Layout**: `css_core::Layouter` receives DOM updates and computed styles to build layout trees and compute positions/sizes
5. **Rendering**: `renderer::Renderer` receives layout information and builds a display list
6. **GPU Drawing**: `wgpu_backend` executes the display list on the GPU
7. **JavaScript Execution**: `js_engine_v8::V8Engine` executes scripts and interacts with DOM via `DOMUpdate` messages

### Key Abstractions

**DOM Mirror Pattern (`DOMMirror<T>`):**
Multiple subsystems maintain synchronized views of the DOM by subscribing to `DOMUpdate` events. Each mirror implements `DOMSubscriber` trait:
- `CSSMirror`: Collects stylesheets from `<style>` elements
- `Orchestrator`: Computes styles for elements
- `Layouter`: Maintains layout tree
- `Renderer`: Maintains scene graph
- `DomIndex`: Indexes elements for JavaScript queries (getElementById, etc.)

**NodeKey System:**
Stable identifiers (`NodeKey`) map between subsystems, allowing each to use their own internal IDs while staying synchronized. `KeySpace` and `NodeKeyManager` coordinate key allocation across mirrors.

**Update Flow in `HtmlPage`:**
The `HtmlPage::update()` method coordinates all subsystems:
1. Process incoming HTML stream chunks
2. Apply DOM updates to all mirrors
3. Recompute styles if dirty
4. Compute layout
5. Build display list for rendering
6. Execute JavaScript tasks
7. Return `UpdateOutcome` indicating if redraw is needed

### Testing Strategy

#### Fixture Tests (Chromium Comparison)
Located in `crates/valor/tests/chromium_compare/`:
- **Run command**: `cargo test --test chromium_tests` (no `--all-features`!)
- **Total fixtures**: 139 HTML test files across CSS modules and application tests
- **Test types**:
  - **Layout comparison**: Compares JSON layout tree (rect positions, dimensions, styles) with 0.6px epsilon
  - **Graphics comparison**: Pixel-level screenshot comparison with region-aware thresholds
- **Caching**: Chrome reference outputs cached in `target/test_cache/{layout,graphics}/` using FNV-1a hash
- **Failure artifacts**: Saved to `target/test_cache/{layout,graphics}/failing/` with:
  - `.error.txt` - Detailed diff of all mismatches
  - `.chrome.json` / `.valor.json` - Reference and actual outputs
  - `.diff.png` - Visual diff (graphics only)

**Current Status (as of Jan 2026)**:
- 81/139 passing (58% pass rate)
- 58/139 failing (42% failure rate)
- Main issues: Text height 3px too short (affects 90% of failures), table display mode not applied

#### Other Tests
- Integration tests in `crates/css/tests/` verify CSS mirror behavior
- Unit tests in CSS modules validate property computation

### Project-Specific Requirements

1. **Always follow CSS and HTML specifications**: Stick to choices that match real browsers like Chromium
2. **Spec-driven development**: CSS modules embed W3C specifications in `spec.md` files with inline status markers:
   - `[Production]`: Feature is complete and tested
   - `[MVP]`: Minimum viable implementation, may need expansion
   - `[Approximation]`: Simplified implementation that approximates spec behavior
   - `[TODO]`: Planned but not yet implemented
   - Each status block includes code location references and fixture paths
3. **Strict linting**: The workspace has `clippy::all = "deny"` with many restriction lints enabled. Most lints are denied by default with specific exceptions
4. **Edition 2024**: Project uses Rust edition 2024
5. **Cranelift for dependencies**: Dev profile uses Cranelift backend for faster dependency compilation
6. **Workspace resolution v3**: Uses new cargo resolver

### Binary Executables

- **`crates/valor/src/main.rs`**: Main browser application
- **`crates/valor/src/bin/layout_compare.rs`**: Layout comparison utility

### Spec Vendoring Scripts

The project includes scripts to vendor (embed) W3C specifications into CSS module `spec.md` files:
- **`./scripts/vendor_display_spec.ps1`** (PowerShell) and **`./scripts/vendor_display_spec.sh`** (Bash)
- Usage: Fetch spec HTML, embed into module spec file with legal notices
- Format: Embedded specs go between `<!-- BEGIN VERBATIM SPEC -->` and `<!-- END VERBATIM SPEC -->` markers
- After vendoring, manually insert status/mapping blocks after each section heading

### Working with CSS Modules

When adding or modifying CSS features:
1. Check the relevant module's `spec.md` for current implementation status
2. Find code locations from the inline status blocks (e.g., `crates/css/modules/core/src/10_visual_details/part_10_3_3_block_widths.rs`)
3. Update status markers when implementation changes
4. Add test fixtures under `crates/css/modules/{module}/tests/fixtures/`
5. Reference specific spec sections in code comments
