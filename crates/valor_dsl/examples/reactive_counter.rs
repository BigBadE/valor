//! Reactive counter example using the new component API
//!
//! This demonstrates the React-like DX with automatic reactivity and minimal boilerplate.

use bevy::prelude::*;
use bevy::window::{Window, WindowPlugin};
use valor_dsl::reactive::prelude::*;
use valor_dsl::reactive::Component;
use valor_dsl::reactive::runtime::ReactiveAppExt;
use valor_dsl::reactive_html;

// Define your state as a simple Bevy component
#[derive(Component)]
struct Counter {
    value: i32,
}

// Implement the Component trait with a render function
impl valor_dsl::reactive::Component for Counter {
    fn render(ui: &mut UiContext<Self>) -> Html {
        let count = ui.use_state().value;

        // Register event handlers - they get mutable access to Counter
        let increment = ui.on_click("increment", |counter: &mut Counter| {
            counter.value += 1;
            info!("✨ Counter incremented to: {}", counter.value);
        });

        let decrement = ui.on_click("decrement", |counter: &mut Counter| {
            counter.value -= 1;
            info!("✨ Counter decremented to: {}", counter.value);
        });

        let reset = ui.on_click("reset", |counter: &mut Counter| {
            counter.value = 0;
            info!("✨ Counter reset to 0");
        });

        // HTML with Rust interpolation
        reactive_html! {
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Reactive Counter</title>
                <style>
                    body {
                        font-family: Arial, sans-serif;
                        padding: 40px;
                        text-align: center;
                        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        color: white;
                        min-height: 100vh;
                        margin: 0;
                    }
                    .counter {
                        font-size: 72px;
                        margin: 40px 0;
                        font-weight: bold;
                        text-shadow: 2px 2px 4px rgba(0,0,0,0.3);
                    }
                    button {
                        font-size: 18px;
                        padding: 15px 30px;
                        margin: 10px;
                        cursor: pointer;
                        border: none;
                        background: white;
                        color: #667eea;
                        border-radius: 8px;
                        font-weight: bold;
                        transition: transform 0.1s, box-shadow 0.1s;
                        box-shadow: 0 4px 6px rgba(0,0,0,0.1);
                    }
                    button:hover {
                        transform: translateY(-2px);
                        box-shadow: 0 6px 12px rgba(0,0,0,0.15);
                    }
                    button:active {
                        transform: translateY(0);
                    }
                    h1 {
                        font-size: 48px;
                        margin-bottom: 20px;
                        text-shadow: 2px 2px 4px rgba(0,0,0,0.3);
                    }
                    p {
                        font-size: 16px;
                        opacity: 0.9;
                    }
                </style>
            </head>
            <body>
                <h1>🚀 Reactive Counter</h1>
                <div class="counter">"#
                    {count}
                r#"</div>
                <div>
                    <button onclick=""# {&increment} r#"">➕ Increment</button>
                    <button onclick=""# {&decrement} r#"">➖ Decrement</button>
                    <button onclick=""# {&reset} r#"">🔄 Reset</button>
                </div>
                <p>✨ React-like components powered by Valor + Bevy!</p>
            </body>
            </html>
            "#
        }
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Just spawn the component - the reactive system handles the rest!
    commands.spawn(Counter { value: 0 });

    info!("🎉 Reactive counter initialized!");
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Reactive Counter".to_string(),
                resolution: (800.0, 600.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(valor_dsl::reactive::ReactiveUiPlugin)
        // Register the Counter component type with its render function
        .add_reactive_component(Counter::render)
        .add_systems(Startup, setup)
        .run();
}
