//! Bevy UI example with real visual counter
//!
//! This opens a real Bevy window and displays a functional counter UI.

use bevy::prelude::*;
use std::sync::{Arc, Mutex};
use valor_dsl::*;
use valor_dsl::events::{EventCallbacks, EventContext};
use js::{KeySpace, NodeKey};

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

    fn get(&self) -> i32 {
        *self.count.lock().unwrap()
    }

    fn increment(&self) {
        let mut count = self.count.lock().unwrap();
        *count += 1;
    }

    fn decrement(&self) {
        let mut count = self.count.lock().unwrap();
        *count -= 1;
    }

    fn reset(&self) {
        let mut count = self.count.lock().unwrap();
        *count = 0;
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

fn setup_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
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
        .with_children(|parent| {
            // Container box
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
                .with_children(|parent| {
                    // Title
                    parent.spawn((
                        Text::new("🎯 Valor Counter"),
                        TextFont {
                            font_size: 42.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.2, 0.2, 0.2)),
                    ));

                    // Counter display
                    parent.spawn((
                        Text::new("0"),
                        TextFont {
                            font_size: 96.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.4, 0.35, 0.65)), // Purple
                        CounterDisplay,
                    ));

                    // Button group
                    parent
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(15.0),
                                margin: UiRect::top(Val::Px(20.0)),
                                ..default()
                            },
                        ))
                        .with_children(|parent| {
                            // Decrement button
                            create_button(
                                parent,
                                "−",
                                Color::srgb(0.95, 0.34, 0.42), // Red gradient
                                DecrementButton,
                            );

                            // Reset button
                            create_button(
                                parent,
                                "↻",
                                Color::srgb(0.31, 0.67, 0.99), // Blue gradient
                                ResetButton,
                            );

                            // Increment button
                            create_button(
                                parent,
                                "+",
                                Color::srgb(0.26, 0.91, 0.48), // Green gradient
                                IncrementButton,
                            );
                        });

                    // Description
                    parent.spawn((
                        Text::new("Built with Valor DSL\nClick buttons to test!"),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.4, 0.4, 0.4)),
                        TextLayout::new_with_justify(JustifyText::Center),
                    ));
                });
        });

    info!("✅ Bevy UI setup complete!");
    info!("🎮 Click the buttons to interact with the counter");
}

fn create_button<T: Component>(
    parent: &mut ChildBuilder,
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

fn handle_button_clicks(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, Option<&IncrementButton>, Option<&DecrementButton>, Option<&ResetButton>),
        Changed<Interaction>,
    >,
    counter_state: Res<CounterState>,
) {
    for (interaction, mut color, increment, decrement, reset) in interaction_query.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                if increment.is_some() {
                    counter_state.increment();
                    info!("➕ Increment! Count: {}", counter_state.get());
                } else if decrement.is_some() {
                    counter_state.decrement();
                    info!("➖ Decrement! Count: {}", counter_state.get());
                } else if reset.is_some() {
                    counter_state.reset();
                    info!("↻ Reset! Count: {}", counter_state.get());
                }
            }
            Interaction::Hovered => {
                // Brighten on hover
                if let Some(_) = increment {
                    *color = BackgroundColor(Color::srgb(0.28, 0.98, 0.55));
                } else if let Some(_) = decrement {
                    *color = BackgroundColor(Color::srgb(1.0, 0.40, 0.48));
                } else if let Some(_) = reset {
                    *color = BackgroundColor(Color::srgb(0.36, 0.74, 1.0));
                }
            }
            Interaction::None => {
                // Reset to original color
                if let Some(_) = increment {
                    *color = BackgroundColor(Color::srgb(0.26, 0.91, 0.48));
                } else if let Some(_) = decrement {
                    *color = BackgroundColor(Color::srgb(0.95, 0.34, 0.42));
                } else if let Some(_) = reset {
                    *color = BackgroundColor(Color::srgb(0.31, 0.67, 0.99));
                }
            }
        }
    }
}

fn update_counter_display(
    mut query: Query<&mut Text, With<CounterDisplay>>,
    counter_state: Res<CounterState>,
) {
    for mut text in query.iter_mut() {
        let new_value = format!("{}", counter_state.get());
        if text.0 != new_value {
            text.0 = new_value;
        }
    }
}
