//! Bevy UI example with real visual counter
//!
//! This opens a real Bevy window and displays a functional counter UI.

use bevy::ecs::system::ChildSpawnerCommands;
use bevy::prelude::*;
use bevy::winit::WinitSettings;
use log::info;
use std::sync::{Arc, Mutex};

/// Main entry point
fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor DSL - Counter Example".into(),
                resolution: (800.0, 600.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(WinitSettings::desktop_app())
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
        .insert_resource(CounterState::new())
        .add_systems(Startup, setup_ui)
        .add_systems(Update, (handle_button_clicks, update_counter_display))
        .run();
}

#[derive(Resource)]
struct CounterState {
    count: Arc<Mutex<i32>>,
}

impl CounterState {
    fn new() -> Self {
        Self {
            count: Arc::new(Mutex::new(0)),
        }
    }

    /// Get the current count value
    fn get(&self) -> Option<i32> {
        self.count.lock().ok().map(|guard| *guard)
    }

    /// Increment the counter by 1
    fn increment(&self) {
        if let Ok(mut count) = self.count.lock() {
            *count += 1;
        }
    }

    /// Decrement the counter by 1
    fn decrement(&self) {
        if let Ok(mut count) = self.count.lock() {
            *count -= 1;
        }
    }

    /// Reset the counter to 0
    fn reset(&self) {
        if let Ok(mut count) = self.count.lock() {
            *count = 0;
        }
    }
}

#[derive(Component)]
struct CounterDisplay;

#[derive(Component)]
struct IncrementButton;

#[derive(Component)]
struct DecrementButton;

#[derive(Component)]
struct ResetButton;

/// Spawn the button group with all three buttons
fn spawn_button_group(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(15.0),
            margin: UiRect::top(Val::Px(20.0)),
            ..default()
        },))
        .with_children(|button_parent| {
            // Decrement button
            create_button(
                button_parent,
                "−",
                Color::srgb(0.95, 0.34, 0.42), // Red gradient
                DecrementButton,
            );

            // Reset button
            create_button(
                button_parent,
                "↻",
                Color::srgb(0.31, 0.67, 0.99), // Blue gradient
                ResetButton,
            );

            // Increment button
            create_button(
                button_parent,
                "+",
                Color::srgb(0.26, 0.91, 0.48), // Green gradient
                IncrementButton,
            );
        });
}

/// Spawn the counter container with title, display, buttons, and description
fn spawn_counter_container(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(60.0)),
                row_gap: Val::Px(30.0),
                ..default()
            },
            BackgroundColor(Color::WHITE),
            BorderRadius::all(Val::Px(20.0)),
        ))
        .with_children(|container_parent| {
            // Title
            container_parent.spawn((
                Text::new("🎯 Valor Counter"),
                TextFont {
                    font_size: 42.0,
                    ..default()
                },
                TextColor(Color::srgb(0.2, 0.2, 0.2)),
            ));

            // Counter display
            container_parent.spawn((
                Text::new("0"),
                TextFont {
                    font_size: 96.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.35, 0.65)), // Purple
                CounterDisplay,
            ));

            spawn_button_group(container_parent);

            // Description
            container_parent.spawn((
                Text::new("Built with Valor DSL\nClick buttons to test!"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.4, 0.4)),
                TextLayout::new_with_justify(JustifyText::Center),
            ));
        });
}

/// Setup the UI
fn setup_ui(mut commands: Commands, _asset_server: Res<AssetServer>) {
    // Spawn camera
    commands.spawn(Camera2d);

    // Create the UI layout
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.4, 0.35, 0.55)), // Purple gradient effect
        ))
        .with_children(spawn_counter_container);

    info!("✅ Bevy UI setup complete!");
    info!("🎮 Click the buttons to interact with the counter");
}

/// Create a button with the specified text, color, and marker component
fn create_button<T: Component>(
    parent: &mut ChildSpawnerCommands,
    text: &str,
    color: Color,
    marker: T,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(120.0),
                height: Val::Px(60.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(color),
            BorderRadius::all(Val::Px(10.0)),
            marker,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(text),
                TextFont {
                    font_size: 32.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

/// Handle button click interactions
fn handle_button_clicks(
    mut interaction_query: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            Option<&IncrementButton>,
            Option<&DecrementButton>,
            Option<&ResetButton>,
        ),
        Changed<Interaction>,
    >,
    counter_state: Res<CounterState>,
) {
    for (interaction, mut color, increment, decrement, reset) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                if increment.is_some() {
                    counter_state.increment();
                    if let Some(count) = counter_state.get() {
                        info!("➕ Increment! Count: {count}");
                    }
                } else if decrement.is_some() {
                    counter_state.decrement();
                    if let Some(count) = counter_state.get() {
                        info!("➖ Decrement! Count: {count}");
                    }
                } else if reset.is_some() {
                    counter_state.reset();
                    if let Some(count) = counter_state.get() {
                        info!("↻ Reset! Count: {count}");
                    }
                }
            }
            Interaction::Hovered => {
                // Brighten on hover
                if increment.is_some() {
                    *color = BackgroundColor(Color::srgb(0.28, 0.98, 0.55));
                } else if decrement.is_some() {
                    *color = BackgroundColor(Color::srgb(1.0, 0.40, 0.48));
                } else if reset.is_some() {
                    *color = BackgroundColor(Color::srgb(0.36, 0.74, 1.0));
                }
            }
            Interaction::None => {
                // Reset to original color
                if increment.is_some() {
                    *color = BackgroundColor(Color::srgb(0.26, 0.91, 0.48));
                } else if decrement.is_some() {
                    *color = BackgroundColor(Color::srgb(0.95, 0.34, 0.42));
                } else if reset.is_some() {
                    *color = BackgroundColor(Color::srgb(0.31, 0.67, 0.99));
                }
            }
        }
    }
}

/// Update the counter display text
fn update_counter_display(
    mut query: Query<&mut Text, With<CounterDisplay>>,
    counter_state: Res<CounterState>,
) {
    for mut text in &mut query {
        if let Some(count) = counter_state.get() {
            let new_value = count.to_string();
            if text.0 != new_value {
                text.0 = new_value;
            }
        }
    }
}
