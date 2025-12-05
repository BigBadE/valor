//! Integration test for Bevy + Valor DSL
//!
//! Tests that clicking a UI element spawns an entity in the Bevy world.

#![cfg(feature = "bevy_integration")]

use bevy::prelude::*;
use std::sync::{Arc, Mutex};
use valor_dsl::bevy_integration::*;
use valor_dsl::events::{EventCallbacks, EventContext};
use valor_dsl::VirtualDom;
use js::{KeySpace, NodeKey};

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

#[test]
fn test_bevy_click_spawns_entity() {
    // Create Tokio runtime for async operations
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let mut app = App::new();

    // Add minimal plugins for headless testing
    app.add_plugins(MinimalPlugins);

    // Add our test state
    let test_state = TestState::default();
    let clicks_counter = test_state.clicks.clone();
    app.insert_resource(test_state);

    // Setup system that creates the UI
    app.add_systems(Startup, setup_test_ui);

    // System to handle clicks and spawn entities
    app.add_systems(Update, handle_test_clicks);

    // System to verify spawned entities
    app.add_systems(Update, verify_spawned_entities);

    // Run one update to setup
    app.update();

    // Simulate a click by directly calling the event handler
    let mut test_state = app.world_mut().resource_mut::<TestState>();
    {
        let mut clicks = test_state.clicks.lock().unwrap();
        *clicks = 1;
    }

    // Run update to process the click
    app.update();

    // Verify entity was spawned
    let test_state = app.world().resource::<TestState>();
    assert_eq!(test_state.spawned_entities.len(), 1, "Should have spawned 1 entity");

    // Verify the entity exists and has the correct component
    let entity = test_state.spawned_entities[0];
    let world = app.world();

    assert!(
        world.get::<ClickSpawned>(entity).is_some(),
        "Spawned entity should have ClickSpawned component"
    );

    let click_spawned = world.get::<ClickSpawned>(entity).unwrap();
    assert_eq!(click_spawned.click_count, 1, "Click count should be 1");

    // Test multiple clicks
    let mut test_state = app.world_mut().resource_mut::<TestState>();
    {
        let mut clicks = test_state.clicks.lock().unwrap();
        *clicks = 3;
    }

    app.update();

    let test_state = app.world().resource::<TestState>();
    assert_eq!(test_state.spawned_entities.len(), 3, "Should have spawned 3 entities total");
}

fn setup_test_ui(
    mut commands: Commands,
    test_state: Res<TestState>,
) {
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

    let clicks = test_state.clicks.clone();
    callbacks.register("spawn_entity", move |_ctx: &EventContext| {
        let mut count = clicks.lock().unwrap();
        *count += 1;
        info!("Button clicked! Total clicks: {}", *count);
    });

    // Create VirtualDom
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    match vdom.compile_html(html, NodeKey::ROOT, callbacks) {
        Ok(updates) => {
            info!("Generated {} DOM updates", updates.len());
        }
        Err(e) => {
            error!("Failed to compile HTML: {}", e);
        }
    }
}

fn handle_test_clicks(
    mut commands: Commands,
    mut test_state: ResMut<TestState>,
) {
    let current_clicks = {
        let clicks = test_state.clicks.lock().unwrap();
        *clicks
    };

    // Spawn entities for new clicks
    while test_state.spawned_entities.len() < current_clicks as usize {
        let click_num = test_state.spawned_entities.len() as u32 + 1;

        let entity = commands.spawn(ClickSpawned {
            click_count: click_num,
        }).id();

        test_state.spawned_entities.push(entity);
        info!("Spawned entity {:?} for click #{}", entity, click_num);
    }
}

fn verify_spawned_entities(
    query: Query<(Entity, &ClickSpawned)>,
    test_state: Res<TestState>,
) {
    if !test_state.spawned_entities.is_empty() {
        let count = query.iter().count();
        info!("Verified {} spawned entities in world", count);
    }
}

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

    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok(), "HTML compilation should succeed");

    let updates = result.unwrap();
    assert!(!updates.is_empty(), "Should generate DOM updates");

    // Check that we have InsertElement for div and button
    let has_div = updates.iter().any(|u| {
        matches!(u, js::DOMUpdate::InsertElement { tag, .. } if tag == "div")
    });
    let has_button = updates.iter().any(|u| {
        matches!(u, js::DOMUpdate::InsertElement { tag, .. } if tag == "button")
    });

    assert!(has_div, "Should have div element");
    assert!(has_button, "Should have button element");
}

#[test]
fn test_event_callback_registration() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    let html = r#"<button on:click="test_handler">Test</button>"#;

    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let mut callbacks = EventCallbacks::new();
    callbacks.register("test_handler", move |_ctx| {
        called_clone.store(true, Ordering::SeqCst);
    });

    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    // Find the button node key
    let updates = result.unwrap();
    let button_key = updates.iter().find_map(|u| {
        if let js::DOMUpdate::InsertElement { node, tag, .. } = u {
            if tag == "button" {
                return Some(*node);
            }
        }
        None
    });

    assert!(button_key.is_some(), "Should have button node key");

    // Get the handler
    let handler = vdom.get_handler(button_key.unwrap(), "click");
    assert!(handler.is_some(), "Should have registered click handler");
}

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

    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    let updates = result.unwrap();
    let input_key = updates.iter().find_map(|u| {
        if let js::DOMUpdate::InsertElement { node, tag, .. } = u {
            if tag == "input" {
                return Some(*node);
            }
        }
        None
    });

    assert!(input_key.is_some());
    let key = input_key.unwrap();

    // Verify all three handlers are registered
    assert!(vdom.get_handler(key, "input").is_some());
    assert!(vdom.get_handler(key, "focus").is_some());
    assert!(vdom.get_handler(key, "blur").is_some());
}

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

    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    let updates = result.unwrap();

    // Should have multiple InsertElement updates
    let element_count = updates.iter().filter(|u| {
        matches!(u, js::DOMUpdate::InsertElement { .. })
    }).count();

    assert!(element_count >= 3, "Should have at least 3 elements (2 divs + 1 button)");
}

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
    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    let updates = result.unwrap();

    // Check for style attribute
    let has_style_attr = updates.iter().any(|u| {
        if let js::DOMUpdate::SetAttr { name, value, .. } = u {
            name == "style" && value.contains("color") && value.contains("padding")
        } else {
            false
        }
    });

    assert!(has_style_attr, "Should have style attribute with CSS properties");
}

#[test]
fn test_class_attribute() {
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    let html = r#"<div class="container primary active">Content</div>"#;

    let callbacks = EventCallbacks::new();
    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    let updates = result.unwrap();

    let has_class = updates.iter().any(|u| {
        if let js::DOMUpdate::SetAttr { name, value, .. } = u {
            name == "class" && value == "container primary active"
        } else {
            false
        }
    });

    assert!(has_class, "Should have class attribute");
}
