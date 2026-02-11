# Valor DSL - Bevy-Integrated HTML UI System

A declarative HTML/CSS UI framework for Bevy applications, powered by the Valor browser engine.

## Overview

Valor DSL allows you to create UIs using standard HTML/CSS that integrate seamlessly with Bevy's ECS and observer system. HTML event attributes like `onclick="increment"` trigger Bevy observer systems directly.

## Quick Start

```rust
use bevy::prelude::*;
use valor_dsl::{html, click_handler, bevy_integration::*, bevy_events::*};

#[derive(Component)]
struct Counter(i32);

fn setup(mut commands: Commands) {
    commands.spawn(Counter(0));

    // Register click handlers - these match the onclick attributes
    click_handler!(commands, increment_counter);

    // Create UI with HTML
    let ui = html! {
        r#"
        <button onclick="increment_counter">Click me!</button>
        "#
    };

    commands.spawn(ValorUi::new(ui));
}

// Bevy observer - triggered when the button is clicked
fn increment_counter(
    _trigger: Trigger<OnClick>,
    mut query: Query<&mut Counter>,
) {
    for mut counter in &mut query {
        counter.0 += 1;
        info!("Count: {}", counter.0);
    }
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, ValorUiPlugin))
        .add_systems(Startup, setup)
        .add_observer(increment_counter)
        .run();
}
```

## How It Works

1. **HTML with Event Attributes**: Write standard HTML with `onclick="function_name"`, `oninput="function_name"`, etc.
2. **Register Handlers**: Use `click_handler!(commands, function_name)` to register observer functions
3. **Observers**: Write Bevy observers that receive `OnClick`, `OnInput`, etc. events
4. **Automatic Wiring**: When HTML elements are clicked, matching Bevy observers are triggered automatically

## Event Types

All events are in the `bevy_events` module:

- `OnClick` - Element clicked
- `OnInput` - Text input occurred
- `OnChange` - Form input value changed
- `OnSubmit` - Form submitted
- `OnFocus` - Element gained focus
- `OnBlur` - Element lost focus
- `OnKeyDown` - Key pressed
- `OnKeyUp` - Key released
- `OnMouseEnter` - Mouse entered element
- `OnMouseLeave` - Mouse left element
- `OnMouseMove` - Mouse moved over element

## Example: Counter App

```rust
fn setup(mut commands: Commands) {
    commands.spawn(Counter(0));

    // Register click handlers
    click_handler!(commands, increment_counter);
    click_handler!(commands, decrement_counter);
    click_handler!(commands, reset_counter);

    let ui = html! {
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                body { font-family: Arial; padding: 40px; text-align: center; }
                .counter { font-size: 48px; margin: 30px; }
                button { font-size: 18px; padding: 15px 30px; margin: 10px; }
            </style>
        </head>
        <body>
            <h1>Counter</h1>
            <div class="counter">Count: <span id="count">0</span></div>
            <button onclick="increment_counter">+</button>
            <button onclick="decrement_counter">-</button>
            <button onclick="reset_counter">Reset</button>
        </body>
        </html>
        "#
    };

    commands.spawn(ValorUi::new(ui).with_width(800).with_height(600));
}

fn increment_counter(_trigger: Trigger<OnClick>, mut q: Query<&mut Counter>) {
    for mut counter in &mut q {
        counter.0 += 1;
    }
}

fn decrement_counter(_trigger: Trigger<OnClick>, mut q: Query<&mut Counter>) {
    for mut counter in &mut q {
        counter.0 -= 1;
    }
}

fn reset_counter(_trigger: Trigger<OnClick>, mut q: Query<&mut Counter>) {
    for mut counter in &mut q {
        counter.0 = 0;
    }
}
```

## Components & Resources

### `ValorUi` (Component)

Marks an entity as hosting a Valor UI.

```rust
let ui = ValorUi::new("<html>...</html>")
    .with_width(1024)
    .with_height(768);
commands.spawn(ui);
```

### `ClickHandler` (Component)

Marks an entity as a handler for click events with a specific name.

```rust
click_handler!(commands, my_click_handler);
// Creates an entity with ClickHandler { name: "my_click_handler" }
```

## HTML Macro

The `html!` macro creates HTML strings:

```rust
let ui = html! {
    r#"
    <div>
        <h1>Title</h1>
        <button onclick="handleClick">Click</button>
    </div>
    "#
};
```

## Integration with Bevy

Valor UI events integrate with Bevy's observer system:

```rust
App::new()
    .add_plugins(ValorUiPlugin)        // Add Valor UI support
    .add_observer(my_click_handler)    // Register observer
    .run();

fn my_click_handler(trigger: Trigger<OnClick>) {
    info!("Clicked node: {:?}", trigger.event().node);
}
```

## Two-Way Data Binding

Update HTML elements from Bevy state changes:

```rust
fn update_counter_display(
    mut commands: Commands,
    counter_query: Query<&Counter, Changed<Counter>>,
) {
    for counter in &counter_query {
        let count = counter.0;
        commands.queue(move |world: &mut World| {
            let valor_ui_entities: Vec<Entity> = world
                .query_filtered::<Entity, With<ValorUi>>()
                .iter(world)
                .collect();

            for entity in valor_ui_entities {
                update_element_text(world, entity, "count", &count.to_string());
            }
        });
    }
}
```

## Public API

### `dispatch_click(world, valor_ui_entity, x, y, button)`
Dispatch a click event at the given coordinates. Call this from input handling code.

### `update_element_text(world, valor_ui_entity, element_id, text)`
Update the text content of an HTML element by ID. Provides Bevy → HTML data binding.

### `get_element_text(world, valor_ui_entity, element_id) -> Option<String>`
Get the current text content of an HTML element by ID.

## Examples

- `examples/bevy_counter.rs` - Complete counter app with increment/decrement/reset
- Run with: `cargo run --example bevy_counter --features bevy_integration`

## Features

- `bevy_integration` - Enable Bevy ECS integration (required for this functionality)

## Architecture

```
HTML (onclick="function_name")
    ↓
Extract onclick attributes from DOM
    ↓
Query for entities with ClickHandler { name: "function_name" }
    ↓
Trigger OnClick event on matching entities
    ↓
Bevy observers receive the event
    ↓
Your game logic executes in standard Bevy ECS
```

## Features

### Reactive Component System ✅

The reactive system provides a React-like API for building UIs with automatic reactivity:

```rust
use valor_dsl::reactive::{Component, UiContext, Html};
use valor_dsl::reactive_html;

#[derive(bevy::prelude::Component)]
struct Counter { value: i32 }

impl Component for Counter {
    fn render(ui: &mut UiContext<Self>) -> Html {
        let count = ui.use_state().value;

        let increment = ui.on_click("increment", |counter| {
            counter.value += 1;
        });

        reactive_html! {
            r#"<h1>Count: "# {count} r#"</h1>
            <button onclick=""# {&increment} r#"">Increment</button>"#
        }
    }
}
```

**Features:**
- ✅ Automatic re-rendering on state changes
- ✅ Type-safe event handlers with closures
- ✅ HTML template interpolation with `reactive_html!` macro
- ✅ Direct component state mutations
- ✅ Full integration with Bevy ECS

### Image Asset Integration ✅

Images can be loaded via Bevy's asset system and referenced in HTML:

```rust
use valor_dsl::bevy_integration::{load_image, get_image_handle, ImageRegistry};

// Load an image asset
let image_entity = load_image(&mut commands, "assets/logo.png");

// Get the handle later
let handle = get_image_handle(&registry, "assets/logo.png");
```

The `ImageRegistry` resource tracks loaded images and provides handles that can be used both in Bevy UI and Valor HTML rendering.

### Examples

- **`reactive_counter.rs`** - React-like counter with increment/decrement/reset ✅
- **`image_gallery.rs`** - Image carousel demonstrating asset integration ✅
- **`bevy_counter.rs`** - Traditional Bevy observer-based counter ✅

Run examples with:
```bash
cargo run --example reactive_counter --features bevy_integration
cargo run --example image_gallery --features bevy_integration
```

## Implementation Status

- [x] Actual HtmlPage integration to render HTML ✅
  - ValorUi components create and manage HtmlPage instances
  - HTML is rendered using the full Valor browser engine
  - Viewport dimensions are configurable
- [x] Event dispatch from DOM to Bevy ✅
  - onclick attributes are extracted from DOM
  - Click events trigger Bevy observers via `dispatch_click()`
  - Direct function name references using `click_handler!` macro
- [x] Two-way data binding (update HTML from Bevy state) ✅
  - `update_element_text()` updates DOM from Bevy
  - `get_element_text()` reads DOM into Bevy
  - Full JavaScript evaluation available via eval_js
- [x] Reactive component system ✅
  - React-like API with `Component` trait and `UiContext`
  - Automatic re-rendering on state changes via change detection
  - Event handlers registered during render
  - HTML template interpolation with `reactive_html!` macro
- [x] Bevy asset system integration ✅
  - `ImageRegistry` resource for managing image assets
  - `load_image()` API for loading images via asset server
  - `get_image_handle()` for retrieving loaded image handles
- [ ] Visual rendering integration
  - HtmlPages are created and rendered to textures
  - Display lists are executed via wgpu_backend
  - Persistent GPU contexts for performance
  - Need: Better integration with Bevy's rendering pipeline
- [ ] More event types (drag & drop, scroll, etc.)
  - Event types defined but dispatch not yet implemented
  - Need: OnInput, OnChange, OnSubmit, OnKeyDown, OnMouseMove, etc.
- [ ] Multiple UI viewports
  - Architecture supports it (multiple ValorUi entities)
  - Needs testing and refinement

## License

Same as Valor project.
