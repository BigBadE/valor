# Valor DSL

A declarative HTML/CSS UI framework for the Valor browser engine with optional Bevy ECS integration.

## Overview

Valor DSL lets you write UIs using standard HTML/CSS syntax that compiles to `DOMUpdate` messages for the Valor browser engine. Perfect for building game UIs in Bevy without the overhead of embedding a full browser.

## Features

- ✅ **Full HTML/CSS Syntax** - Write real HTML and CSS, no proprietary DSL to learn
- ✅ **Zero-Cost Abstraction** - Compiles to efficient `DOMUpdate` messages
- ✅ **Event Callbacks** - Rust closures for event handling via `on:*` attributes
- ✅ **Bevy Integration** - Optional ECS integration for game UIs
- ✅ **Type-Safe** - Full Rust type safety with html5ever parsing
- ✅ **Valor Types** - Reuses existing Valor types (`NodeKey`, `DOMUpdate`, `ComputedStyle`)

## 🚀 Quick Start - See It Running!

### Visual Demo (Recommended)

Open a real window with a working counter UI:

```bash
cd crates/valor_dsl
cargo run --example bevy_ui --features bevy_integration --release
```

**What you'll see:**
- 🪟 Real window opens (800x600)
- 🎨 Beautiful purple gradient background
- 🔘 Three working buttons (-, ↻, +)
- 📊 Live counter updates
- ✨ Hover effects on buttons

**Note:** First compile takes 5-10 minutes. Use `--release` for smooth performance!

### Fast Test (Terminal Only)

See the DSL in action without a window:

```bash
cd crates/valor_dsl
cargo run --example simple_counter
```

Shows how HTML compiles to DOM updates.

---

## Quick Start

### Basic Usage

```rust
use valor_dsl::*;
use js::KeySpace;

let mut keyspace = KeySpace::new();
let key_manager = keyspace.register_manager();
let mut vdom = VirtualDom::new(key_manager);

let html = r#"
    <div class="container" style="padding: 20px;">
        <h1>Hello Valor!</h1>
        <button on:click="handle_click">Click Me</button>
    </div>
"#;

let mut callbacks = EventCallbacks::new();
callbacks.register("handle_click", |ctx| {
    println!("Button clicked!");
});

let updates = vdom.compile_html(html, NodeKey::ROOT, callbacks)?;
```

### With Bevy (Optional)

Enable the `bevy_integration` feature:

```toml
[dependencies]
valor_dsl = { path = "crates/valor_dsl", features = ["bevy_integration"] }
```

```rust
use bevy::prelude::*;
use valor_dsl::bevy_integration::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ValorUiPlugin)
        .run();
}
```

## Examples

Run the examples with:

```bash
cargo run --example counter --features bevy_integration
cargo run --example todo_list --features bevy_integration
cargo run --example styled_form --features bevy_integration
```

### Counter Example

A simple counter with increment/decrement/reset buttons showcasing event handling.

### Todo List Example

Full-featured todo list with add/delete/toggle functionality.

### Styled Form Example

Contact form with beautiful CSS styling and form validation.

## Architecture

```
HTML String → html5ever Parser → DOMUpdate Messages → Valor Engine
```

1. **Parsing**: Uses `html5ever` to parse HTML into a DOM tree
2. **Compilation**: Walks the tree and generates `DOMUpdate` messages
3. **Event Registration**: Extracts `on:*` attributes and registers callbacks
4. **Rendering**: Valor engine processes updates and renders with WGPU

## Event Handling

Use `on:*` attributes for events:

```html
<button on:click="increment">+</button>
<input on:input="handle_text" />
<form on:submit="submit_form" />
```

Register handlers:

```rust
let mut callbacks = EventCallbacks::new();

callbacks.register("increment", |ctx| {
    // Access node, event type, DOM sender
    println!("Node: {:?}", ctx.node);
});
```

## Bevy Integration

The Bevy integration provides:

- `ValorUiRoot` component for managing UI instances
- `ValorClickEvent` and `ValorInputEvent` for interaction
- Automatic rendering to Bevy textures
- Bevy asset loading for images/fonts (TODO)

## Comparison with Other Solutions

| Feature | Valor DSL | Webview | Native Bevy UI |
|---------|-----------|---------|----------------|
| HTML/CSS | ✅ Full | ✅ Full | ❌ Custom DSL |
| Performance | ⚡ Fast | 🐌 Slow | ⚡ Fastest |
| Bundle Size | 📦 Small | 📦 Large | 📦 Tiny |
| Web Standards | ✅ Yes | ✅ Yes | ❌ No |
| Rust Integration | ✅ Native | ⚠️ Bridge | ✅ Native |

## Limitations

Current limitations (PRs welcome!):

- JavaScript execution not yet exposed
- No dynamic re-rendering (manual DOM updates only)
- Asset loading via Bevy not implemented
- Limited CSS property support (matches Valor engine)

## Contributing

This crate is part of the Valor browser engine. Contributions welcome!

## License

Same as Valor browser engine.
