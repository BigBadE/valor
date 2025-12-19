//! Bevy-integrated counter example using Valor DSL
//!
//! This example shows how Valor UI events integrate with Bevy's ECS and observer system.

use bevy::prelude::*;
use bevy::window::{Window, WindowPlugin};
use valor_dsl::{bevy_events::*, bevy_integration::*, click_handler, jsx};

#[derive(Component)]
struct Counter(i32);

fn setup(mut commands: Commands) {
    // Spawn a 2D camera to view the UI
    commands.spawn(Camera2d);

    // Spawn counter component
    let initial_count = 0;
    commands.spawn(Counter(initial_count));

    // Register click handlers - these match the onclick attributes in HTML
    let increment_handler = "increment_counter";
    let decrement_handler = "decrement_counter";
    let reset_handler = "reset_counter";

    click_handler!(commands, increment_counter);
    click_handler!(commands, decrement_counter);
    click_handler!(commands, reset_counter);

    // Create the UI with JSX syntax - showcasing dynamic attributes and expressions
    let ui_html = jsx! {
        <div>
            <style>"
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
            "</style>
            <h1>"🚀 Bevy Counter (Observer Pattern)"</h1>
            <div class="counter">
                <span id="count">{initial_count}</span>
            </div>
            <div>
                <button onclick={increment_handler}>"➕ Increment"</button>
                <button onclick={decrement_handler}>"➖ Decrement"</button>
                <button onclick={reset_handler}>"🔄 Reset"</button>
            </div>
            <p>"✨ Click events trigger Bevy observer systems!"</p>
        </div>
    };

    commands.spawn(ValorUi::new(ui_html.content).with_width(800).with_height(600));

    info!("✅ Counter app initialized!");
    info!("   HTML onclick attributes are wired to Bevy observers");
}

// Bevy observer systems - these get triggered by UI click events
fn increment_counter(
    trigger: Trigger<OnClick>,
    click_handlers: Query<&ClickHandler>,
    mut counter_query: Query<&mut Counter>,
    mut commands: Commands,
) {
    // Only proceed if this observer was triggered on the correct handler entity
    let Ok(handler) = click_handlers.get(trigger.entity()) else { return };
    if handler.name != "increment_counter" {
        return;
    }

    for mut counter in &mut counter_query {
        counter.0 += 1;
        info!("✨ Counter incremented to: {}", counter.0);

        // Schedule the UI update to happen after this observer
        let count_value = counter.0;
        commands.queue(move |world: &mut World| {
            // Find the ValorUi entity (in a real app, you'd track this)
            let valor_ui_entities: Vec<Entity> = world
                .query_filtered::<Entity, With<ValorUi>>()
                .iter(world)
                .collect();

            for entity in valor_ui_entities {
                valor_dsl::bevy_integration::update_element_text(
                    world,
                    entity,
                    "count",
                    &count_value.to_string(),
                );
            }
        });
    }
}

fn decrement_counter(
    trigger: Trigger<OnClick>,
    click_handlers: Query<&ClickHandler>,
    mut counter_query: Query<&mut Counter>,
    mut commands: Commands,
) {
    // Only proceed if this observer was triggered on the correct handler entity
    let Ok(handler) = click_handlers.get(trigger.entity()) else { return };
    if handler.name != "decrement_counter" {
        return;
    }

    for mut counter in &mut counter_query {
        counter.0 -= 1;
        info!("✨ Counter decremented to: {}", counter.0);

        let count_value = counter.0;
        commands.queue(move |world: &mut World| {
            let valor_ui_entities: Vec<Entity> = world
                .query_filtered::<Entity, With<ValorUi>>()
                .iter(world)
                .collect();

            for entity in valor_ui_entities {
                valor_dsl::bevy_integration::update_element_text(
                    world,
                    entity,
                    "count",
                    &count_value.to_string(),
                );
            }
        });
    }
}

fn reset_counter(
    trigger: Trigger<OnClick>,
    click_handlers: Query<&ClickHandler>,
    mut counter_query: Query<&mut Counter>,
    mut commands: Commands,
) {
    // Only proceed if this observer was triggered on the correct handler entity
    let Ok(handler) = click_handlers.get(trigger.entity()) else { return };
    if handler.name != "reset_counter" {
        return;
    }

    for mut counter in &mut counter_query {
        counter.0 = 0;
        info!("✨ Counter reset to 0");

        commands.queue(move |world: &mut World| {
            let valor_ui_entities: Vec<Entity> = world
                .query_filtered::<Entity, With<ValorUi>>()
                .iter(world)
                .collect();

            for entity in valor_ui_entities {
                valor_dsl::bevy_integration::update_element_text(world, entity, "count", "0");
            }
        });
    }
}

fn main() {
    info!("Starting Bevy Counter Example");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Counter - Bevy Integration".to_string(),
                resolution: (800.0, 600.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(ValorUiPlugin)
        .add_systems(Startup, setup)
        // Register observers for UI events (Bevy 0.15 syntax)
        .add_observer(increment_counter)
        .add_observer(decrement_counter)
        .add_observer(reset_counter)
        .run();
}
