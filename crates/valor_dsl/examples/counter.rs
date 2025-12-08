//! Counter example using Valor DSL with Bevy

use bevy::prelude::*;
use bevy::winit::WinitSettings;
use log::error;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use valor_dsl::events::{EventCallbacks, EventContext};

/// Main entry point for the counter example
fn main() {
    // Create Tokio runtime for async operations
    let Ok(runtime) = Runtime::new() else {
        error!("Failed to create Tokio runtime");
        return;
    };
    let _guard = runtime.enter();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Counter".into(),
                resolution: (1024.0, 768.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(WinitSettings::desktop_app())
        .insert_resource(CounterState {
            count: Arc::new(Mutex::new(0)),
        })
        .add_systems(Startup, setup_counter)
        .add_systems(Update, handle_counter_clicks)
        .run();
}

#[derive(Resource, Clone)]
struct CounterState {
    count: Arc<Mutex<i32>>,
}

fn create_event_callbacks(counter_state: &CounterState) -> EventCallbacks {
    let mut callbacks = EventCallbacks::new();

    let increment_count = Arc::clone(&counter_state.count);
    callbacks.register("increment", move |_ctx: &EventContext| {
        let Ok(mut count) = increment_count.lock() else {
            error!("Failed to lock count mutex");
            return;
        };
        *count += 1;
        info!("Count incremented to: {count}");
    });

    let decrement_count = Arc::clone(&counter_state.count);
    callbacks.register("decrement", move |_ctx: &EventContext| {
        let Ok(mut count) = decrement_count.lock() else {
            error!("Failed to lock count mutex");
            return;
        };
        *count -= 1;
        info!("Count decremented to: {count}");
    });

    let reset_count = Arc::clone(&counter_state.count);
    callbacks.register("reset", move |_ctx: &EventContext| {
        let Ok(mut count) = reset_count.lock() else {
            error!("Failed to lock count mutex");
            return;
        };
        *count = 0;
        info!("Count reset to: 0");
    });

    callbacks
}

/// Setup the counter UI
fn setup_counter(mut commands: Commands, counter_state: Res<CounterState>) {
    commands.spawn(Camera2d);
    let _callbacks = create_event_callbacks(&counter_state);
    info!("Counter app started with HTML UI");
}

/// Handle mouse clicks to increment counter
fn handle_counter_clicks(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    counter_state: Res<CounterState>,
) {
    if mouse_button.just_pressed(MouseButton::Left) && windows.get_single().is_ok() {
        // Simplified: just increment on any click for demo
        let Ok(mut count) = counter_state.count.lock() else {
            error!("Failed to lock count mutex");
            return;
        };
        *count += 1;
        info!("Count: {count}");
    }
}
