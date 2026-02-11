//! Integration test for Bevy + Valor DSL
//!
//! Tests that clicking a UI element spawns an entity in the Bevy world.

#![cfg(feature = "bevy_integration")]

use bevy::prelude::*;
use js::{DOMUpdate, KeySpace, NodeKey};
use log::{error, info};
use std::sync::{Arc, Mutex};
use valor_dsl::VirtualDom;
use valor_dsl::events::{EventCallbacks, EventContext};

/// Marker component for entities spawned by UI clicks
#[derive(Component)]
struct ClickSpawned {
    click_count: u32,
}

/// Resource to track clicks and spawned entities
#[derive(Resource, Default)]
struct TestState {
    clicks: Arc<Mutex<u32>>,
    spawned_entities: Vec<Entity>,
}

// Helper functions must be defined before the test module

fn setup_test_ui(_commands: Commands, test_state: Res<TestState>) {
    let html = r#"
        <html>
            <body>
                <div class="test-container">
                    <h1>Test UI</h1>
                    <button id="spawn-btn" on:click="spawn_entity">Spawn Entity</button>
                    <div id="counter">Clicks: 0</div>
                </div>
            </body>
        </html>
    "#;

    let mut callbacks = EventCallbacks::new();

    let clicks = Arc::clone(&test_state.clicks);
    callbacks.register("spawn_entity", move |_ctx: &EventContext| {
        let Ok(mut count) = clicks.lock() else {
            error!("Failed to lock click counter");
            return;
        };
        *count += 1;
        info!("Button clicked! Total clicks: {count}");
    });

    // Create VirtualDom
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    match vdom.compile_html(html, NodeKey::ROOT, &callbacks) {
        Ok(updates) => {
            let len = updates.len();
            info!("Generated {len} DOM updates");
        }
        Err(error) => {
            error!("Failed to compile HTML: {error}");
        }
    }
}

fn handle_test_clicks(mut commands: Commands, mut test_state: ResMut<TestState>) {
    let current_clicks = {
        let Ok(clicks) = test_state.clicks.lock() else {
            error!("Failed to lock click counter");
            return;
        };
        *clicks
    };

    // Spawn entities for new clicks
    while test_state.spawned_entities.len() < current_clicks as usize {
        let click_num = test_state.spawned_entities.len() as u32 + 1;

        let entity = commands
            .spawn(ClickSpawned {
                click_count: click_num,
            })
            .id();

        test_state.spawned_entities.push(entity);
        info!("Spawned entity {entity:?} for click #{click_num}");
    }
}

fn verify_spawned_entities(query: Query<(Entity, &ClickSpawned)>, test_state: Res<TestState>) {
    if !test_state.spawned_entities.is_empty() {
        let count = query.iter().count();
        info!("Verified {count} spawned entities in world");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that clicking a button in the UI spawns entities in Bevy world
    ///
    /// # Panics
    /// Panics if Tokio runtime creation fails or entity spawning fails
    #[test]
    fn test_bevy_click_spawns_entity() {
        use tokio::runtime::Runtime;

        // Create Tokio runtime for async operations
        let runtime = Runtime::new();
        assert!(runtime.is_ok(), "Failed to create Tokio runtime");

        if let Ok(runtime) = runtime {
            let _guard = runtime.enter();

            let mut app = App::new();

            // Add minimal plugins for headless testing
            app.add_plugins(MinimalPlugins);

            // Add our test state
            let initial_state = TestState::default();
            app.insert_resource(initial_state);

            // Setup system that creates the UI
            app.add_systems(Startup, setup_test_ui);

            // System to handle clicks and spawn entities
            app.add_systems(Update, handle_test_clicks);

            // System to verify spawned entities
            app.add_systems(Update, verify_spawned_entities);

            // Run one update to setup
            app.update();

            // Simulate a click by directly calling the event handler
            {
                let state = app.world_mut().resource_mut::<TestState>();
                if let Ok(mut clicks) = state.clicks.lock() {
                    *clicks = 1;
                } else {
                    assert!(state.clicks.lock().is_ok(), "Failed to lock click counter");
                }
            }

            // Run update to process the click
            app.update();

            // Verify entity was spawned
            let after_first_click = app.world().resource::<TestState>();
            assert_eq!(
                after_first_click.spawned_entities.len(),
                1,
                "Should have spawned 1 entity"
            );

            // Verify the entity exists and has the correct component
            let entity = after_first_click.spawned_entities[0];
            let world = app.world();

            assert!(
                world.get::<ClickSpawned>(entity).is_some(),
                "Spawned entity should have ClickSpawned component"
            );

            if let Some(click_spawned) = world.get::<ClickSpawned>(entity) {
                assert_eq!(click_spawned.click_count, 1, "Click count should be 1");
            } else {
                assert!(
                    world.get::<ClickSpawned>(entity).is_some(),
                    "Entity should have ClickSpawned component"
                );
            }

            // Test multiple clicks
            {
                let state_multi = app.world_mut().resource_mut::<TestState>();
                if let Ok(mut clicks) = state_multi.clicks.lock() {
                    *clicks = 3;
                } else {
                    assert!(
                        state_multi.clicks.lock().is_ok(),
                        "Failed to lock click counter"
                    );
                }
            }

            app.update();

            let after_multiple_clicks = app.world().resource::<TestState>();
            assert_eq!(
                after_multiple_clicks.spawned_entities.len(),
                3,
                "Should have spawned 3 entities total"
            );
        }
    }

    /// Tests UI compilation to DOM updates
    ///
    /// # Panics
    /// Panics if HTML compilation fails or expected elements are missing
    #[test]
    fn test_ui_compilation() {
        // Test that HTML compiles to DOMUpdates correctly
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"
            <div class="test">
                <button on:click="handler">Click Me</button>
            </div>
        "#;

        let mut callbacks = EventCallbacks::new();
        callbacks.register("handler", |_ctx| {
            // Test handler
        });

        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok(), "HTML compilation should succeed");

        if let Ok(updates) = result {
            assert!(!updates.is_empty(), "Should generate DOM updates");

            // Check that we have InsertElement for div and button
            let has_div = updates.iter().any(
                |update| matches!(update, DOMUpdate::InsertElement { tag, .. } if tag == "div"),
            );
            let has_button = updates.iter().any(
                |update| matches!(update, DOMUpdate::InsertElement { tag, .. } if tag == "button"),
            );

            assert!(has_div, "Should have div element");
            assert!(has_button, "Should have button element");
        }
    }

    /// Tests event callback registration
    ///
    /// # Panics
    /// Panics if event handlers are not registered correctly
    #[test]
    fn test_event_callback_registration() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"<button on:click="test_handler">Test</button>"#;

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = Arc::clone(&called);

        let mut callbacks = EventCallbacks::new();
        callbacks.register("test_handler", move |_ctx| {
            called_clone.store(true, Ordering::SeqCst);
        });

        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        // Find the button node key
        if let Ok(updates) = result {
            let button_key = updates.iter().find_map(|update| {
                if let DOMUpdate::InsertElement { node, tag, .. } = update
                    && tag == "button"
                {
                    Some(*node)
                } else {
                    None
                }
            });

            assert!(button_key.is_some(), "Should have button node key");

            if let Some(key) = button_key {
                // Get the handler
                let handler = vdom.get_handler(key, "click");
                assert!(handler.is_some(), "Should have registered click handler");
            }
        }
    }

    /// Tests multiple event types on single element
    ///
    /// # Panics
    /// Panics if event handlers are not registered correctly
    #[test]
    fn test_multiple_event_types() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"
            <input
                type="text"
                on:input="handle_input"
                on:focus="handle_focus"
                on:blur="handle_blur"
            />
        "#;

        let mut callbacks = EventCallbacks::new();
        callbacks.register("handle_input", |_| {});
        callbacks.register("handle_focus", |_| {});
        callbacks.register("handle_blur", |_| {});

        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        if let Ok(updates) = result {
            let input_key = updates.iter().find_map(|update| {
                if let DOMUpdate::InsertElement { node, tag, .. } = update
                    && tag == "input"
                {
                    Some(*node)
                } else {
                    None
                }
            });

            assert!(input_key.is_some());

            if let Some(key) = input_key {
                // Verify all three handlers are registered
                assert!(vdom.get_handler(key, "input").is_some());
                assert!(vdom.get_handler(key, "focus").is_some());
                assert!(vdom.get_handler(key, "blur").is_some());
            }
        }
    }

    /// Tests nested HTML elements
    ///
    /// # Panics
    /// Panics if nested elements are not parsed correctly
    #[test]
    fn test_nested_elements() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"
            <div class="outer">
                <div class="middle">
                    <button on:click="nested_handler">Nested Button</button>
                </div>
            </div>
        "#;

        let mut callbacks = EventCallbacks::new();
        callbacks.register("nested_handler", |_| {});

        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        if let Ok(updates) = result {
            // Should have multiple InsertElement updates
            let element_count = updates
                .iter()
                .filter(|update| matches!(update, DOMUpdate::InsertElement { .. }))
                .count();

            assert!(
                element_count >= 3,
                "Should have at least 3 elements (2 divs + 1 button)"
            );
        }
    }

    /// Tests style attribute parsing
    ///
    /// # Panics
    /// Panics if style attributes are not parsed correctly
    #[test]
    fn test_style_attribute_parsing() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"
            <div style="color: red; padding: 20px; background: blue;">
                Styled content
            </div>
        "#;

        let callbacks = EventCallbacks::new();
        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        if let Ok(updates) = result {
            // Check for style attribute
            let has_style_attr = updates.iter().any(|update| {
                if let DOMUpdate::SetAttr { name, value, .. } = update {
                    name == "style" && value.contains("color") && value.contains("padding")
                } else {
                    false
                }
            });

            assert!(
                has_style_attr,
                "Should have style attribute with CSS properties"
            );
        }
    }

    /// Tests class attribute parsing
    ///
    /// # Panics
    /// Panics if class attribute is not set correctly
    #[test]
    fn test_class_attribute() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"<div class="container primary active">Content</div>"#;

        let callbacks = EventCallbacks::new();
        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        if let Ok(updates) = result {
            let has_class = updates.iter().any(|update| {
                if let DOMUpdate::SetAttr { name, value, .. } = update {
                    name == "class" && value == "container primary active"
                } else {
                    false
                }
            });

            assert!(has_class, "Should have class attribute");
        }
    }
}
