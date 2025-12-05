//! Counter example using Valor DSL with Bevy

use bevy::prelude::*;
use std::sync::{Arc, Mutex};
use valor_dsl::bevy_integration::*;
use valor_dsl::events::{EventCallbacks, EventContext};

fn main() {
    // Create Tokio runtime for async operations
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Counter".into(),
                resolution: (1024.0, 768.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(CounterState { count: Arc::new(Mutex::new(0)) })
        .add_systems(Startup, setup_counter)
        .add_systems(Update, handle_counter_clicks)
        .run();
}

#[derive(Resource, Clone)]
struct CounterState {
    count: Arc<Mutex<i32>>,
}

fn setup_counter(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    counter_state: Res<CounterState>,
) {
    commands.spawn(Camera2d);

    let html = r#"
        <html>
            <head>
                <style>
                    body {
                        display: flex;
                        justify-content: center;
                        align-items: center;
                        height: 100vh;
                        margin: 0;
                        background-color: #f0f0f0;
                        font-family: sans-serif;
                    }
                    .container {
                        text-align: center;
                        background: white;
                        padding: 40px;
                        border-radius: 10px;
                        box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
                    }
                    h1 {
                        color: #333;
                        margin-bottom: 20px;
                    }
                    .count {
                        font-size: 72px;
                        font-weight: bold;
                        color: #4CAF50;
                        margin: 30px 0;
                    }
                    .button-group {
                        display: flex;
                        gap: 10px;
                        justify-content: center;
                    }
                    button {
                        padding: 12px 24px;
                        font-size: 16px;
                        border: none;
                        border-radius: 5px;
                        cursor: pointer;
                        background-color: #4CAF50;
                        color: white;
                        transition: background-color 0.3s;
                    }
                    button:hover {
                        background-color: #45a049;
                    }
                    .decrement {
                        background-color: #f44336;
                    }
                    .decrement:hover {
                        background-color: #da190b;
                    }
                    .reset {
                        background-color: #2196F3;
                    }
                    .reset:hover {
                        background-color: #0b7dda;
                    }
                </style>
            </head>
            <body>
                <div class="container">
                    <h1>Counter Example</h1>
                    <div class="count">0</div>
                    <div class="button-group">
                        <button class="decrement" on:click="decrement">Decrement</button>
                        <button class="reset" on:click="reset">Reset</button>
                        <button on:click="increment">Increment</button>
                    </div>
                </div>
            </body>
        </html>
    "#;

    let mut callbacks = EventCallbacks::new();

    let count_clone = counter_state.count.clone();
    callbacks.register("increment", move |_ctx: &EventContext| {
        let mut count = count_clone.lock().unwrap();
        *count += 1;
        info!("Count incremented to: {}", *count);
    });

    let count_clone = counter_state.count.clone();
    callbacks.register("decrement", move |_ctx: &EventContext| {
        let mut count = count_clone.lock().unwrap();
        *count -= 1;
        info!("Count decremented to: {}", *count);
    });

    let count_clone = counter_state.count.clone();
    callbacks.register("reset", move |_ctx: &EventContext| {
        let mut count = count_clone.lock().unwrap();
        *count = 0;
        info!("Count reset to: 0");
    });

    // Create Valor UI (simplified for now)
    info!("Counter app started with HTML UI");
}

fn handle_counter_clicks(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    counter_state: Res<CounterState>,
) {
    if mouse_button.just_pressed(MouseButton::Left) {
        if let Ok(_window) = windows.get_single() {
            // Simplified: just increment on any click for demo
            let mut count = counter_state.count.lock().unwrap();
            *count += 1;
            info!("Count: {}", *count);
        }
    }
}
