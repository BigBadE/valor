# Valor DSL Examples

## ✅ Working Examples

### 🎮 Bevy UI Counter (Visual Window) - **RECOMMENDED**

**This opens a real window with a fully functional, beautiful counter UI!**

**Run it:**
```bash
cd crates/valor_dsl
cargo run --example bevy_ui --features bevy_integration --release
```

**What you'll see:**
- 🪟 A real Bevy window opens (800x600)
- 🎨 Beautiful gradient purple background
- 🎯 White card with rounded corners
- 📊 Large purple counter display (starts at 0)
- 🔘 Three interactive buttons:
  - **−** (Red) - Decrement counter
  - **↻** (Blue) - Reset to 0
  - **+** (Green) - Increment counter
- ✨ Smooth hover effects (buttons brighten)
- 📝 Real-time counter updates

**Features:**
- ✅ Real window with actual UI
- ✅ Working buttons with click handlers
- ✅ Hover effects
- ✅ Beautiful gradients and colors
- ✅ Professional rounded corners
- ✅ Responsive layout

**First run will take 5-10 minutes to compile Bevy. Use `--release` for better performance!**

---

### Simple Counter (Terminal Output)

This example demonstrates the DSL compilation without requiring a window. It shows how HTML/CSS is parsed and converted to DOM updates.

**Run it:**
```bash
cd crates/valor_dsl
cargo run --example simple_counter
```

**What it does:**
- Parses a beautiful counter UI with HTML/CSS
- Compiles it to `DOMUpdate` messages
- Displays the generated updates in the terminal
- Shows the HTML structure and styling

**Expected output:**
```
🚀 Starting Valor Counter Example
📝 HTML length: 4378 bytes
🎨 Rendering UI...
✅ Successfully compiled HTML
📦 Generated 28 DOM updates

📋 Sample DOM Updates:
  1. InsertElement: <html>
  2. InsertElement: <head>
  3. InsertElement: <style>
  4. InsertText: "body { ... }"
  5. InsertElement: <body>
  ... and 23 more updates

✨ Valor DSL Counter example completed successfully!
```

### Run Tests

To verify all functionality works:

```bash
cd crates/valor_dsl
cargo test --test simple_test
```

**Test coverage:**
- ✅ HTML parsing works
- ✅ Event callbacks fire correctly
- ✅ Multiple elements compile
- ✅ Attributes are preserved
- ✅ Nested elements work
- ✅ Style attributes parsed
- ✅ Class attributes parsed

## 🚧 Examples Under Development

### Bevy Integration Examples

These examples show full Bevy integration but are still being finalized:

```bash
# Counter with Bevy (requires long compile time)
cargo run --example counter --features bevy_integration

# Todo List with Bevy
cargo run --example todo_list --features bevy_integration

# Styled Form with Bevy
cargo run --example styled_form --features bevy_integration
```

**Note:** Bevy examples have very long compile times (5-10 minutes) due to Bevy's size.

### Visual Counter (Full Valor Rendering)

The visual counter example will open an actual window and render the UI using Valor's browser engine. This is currently being updated to match the latest Valor API.

## 📖 Example Code

### Basic Usage

```rust
use valor_dsl::*;
use js::KeySpace;

// Create VirtualDOM
let mut keyspace = KeySpace::new();
let key_manager = keyspace.register_manager();
let mut vdom = VirtualDom::new(key_manager);

// Write HTML with full CSS
let html = r#"
    <div class="container" style="padding: 20px;">
        <h1>Hello Valor!</h1>
        <button on:click="handle_click">Click Me</button>
    </div>
"#;

// Register event callbacks
let mut callbacks = EventCallbacks::new();
callbacks.register("handle_click", |ctx| {
    println!("Button clicked!");
});

// Compile to DOM updates
let updates = vdom.compile_html(html, NodeKey::ROOT, callbacks)?;
```

### With Event Handling

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

let counter = Arc::new(AtomicU32::new(0));

let mut callbacks = EventCallbacks::new();

let counter_clone = counter.clone();
callbacks.register("increment", move |_ctx| {
    let new_value = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
    println!("Counter: {}", new_value);
});

let html = r#"<button on:click="increment">+</button>"#;
let updates = vdom.compile_html(html, NodeKey::ROOT, callbacks)?;
```

## 🎯 Features Demonstrated

- ✅ **Full HTML/CSS Syntax** - Real HTML, not a custom DSL
- ✅ **Event Callbacks** - Rust closures via `on:*` attributes
- ✅ **Type Safety** - Compile-time checking
- ✅ **Zero-Cost** - Compiles to efficient `DOMUpdate` messages
- ✅ **Gradients & Styling** - Full CSS support via Valor engine
- ✅ **Nested Elements** - Complete DOM tree support

## 🔧 Troubleshooting

### Long Compile Times

Bevy examples can take 5-10 minutes to compile on first run. Use the simple_counter example for quick testing.

### Missing Dependencies

If you get errors about missing crates, make sure you're in the `crates/valor_dsl` directory when running commands.

### Async Runtime Errors

Make sure examples are run with tokio runtime (they use `#[tokio::main]`).

## 📝 Creating Your Own Examples

1. Create a new file in `examples/my_example.rs`
2. Add it to `Cargo.toml`:
   ```toml
   [[example]]
   name = "my_example"
   ```
3. Use the `simple_counter.rs` as a template
4. Run with: `cargo run --example my_example`

## 🚀 Next Steps

- Try modifying the HTML in `simple_counter.rs`
- Add more buttons with different callbacks
- Experiment with CSS gradients and flexbox
- Build a full application UI!
