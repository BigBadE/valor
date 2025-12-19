//! Runtime system for reactive components

use super::{Component, ComponentFn, UiContext};
use crate::bevy_events::*;
use crate::bevy_integration::*;
use bevy::prelude::*;
use std::any::TypeId;
use std::collections::HashMap;

/// Plugin for reactive UI components
pub struct ReactiveUiPlugin;

impl Plugin for ReactiveUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ValorUiPlugin)
            .insert_resource(ComponentRegistry::default());
    }
}

/// Registry of component render functions
#[derive(Resource, Default)]
pub struct ComponentRegistry {
    /// Map from TypeId to type-erased render function
    renderers: HashMap<TypeId, Box<dyn Fn(Entity, &World) -> String + Send + Sync>>,
}

impl ComponentRegistry {
    /// Register a component type with its render function
    pub fn register<T: Component>(&mut self, render_fn: ComponentFn<T>) {
        let type_id = TypeId::of::<T>();
        let renderer = Box::new(move |entity: Entity, world: &World| {
            // Get the component state
            if let Some(state) = world.get::<T>(entity) {
                // Create UiContext
                let mut ctx = UiContext::new(state, entity, world);
                // Call render function
                let html = render_fn(&mut ctx);
                html.content
            } else {
                String::new()
            }
        });
        self.renderers.insert(type_id, renderer);
    }

    /// Get renderer for a type
    pub fn get_renderer(&self, type_id: TypeId) -> Option<&(dyn Fn(Entity, &World) -> String + Send + Sync)> {
        self.renderers.get(&type_id).map(|b| &**b)
    }
}

/// Extension trait for App to add reactive components
pub trait ReactiveAppExt {
    /// Add a reactive component to the app
    fn add_reactive_component<T: Component>(&mut self, render_fn: ComponentFn<T>) -> &mut Self;
}

impl ReactiveAppExt for App {
    fn add_reactive_component<T: Component>(&mut self, render_fn: ComponentFn<T>) -> &mut Self {
        // Register the component type
        {
            let mut registry = self.world_mut().resource_mut::<ComponentRegistry>();
            registry.register(render_fn);
        }

        // Add initialization system for this specific component type
        // Run in Update instead of Startup to ensure the component is added first
        self.add_systems(
            Update,
            initialize_component::<T>
        );

        // Add observer for click events on this component type
        self.add_observer(handle_click_events::<T>);

        // Add system to register click handlers from reactive callbacks
        self.add_systems(Update, register_click_handlers::<T>);

        // Add change detection system for auto-updates
        self.add_systems(Update, detect_changes::<T>);

        self
    }
}

/// Initialize a component by rendering it and setting up UI
fn initialize_component<T: Component>(world: &mut World) {
    // Collect entities to initialize
    let entities_to_init: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, (With<T>, Without<ValorUi>)>();
        query.iter(world).collect()
    };

    info!("initialize_component: Found {} entities to initialize for type {}",
        entities_to_init.len(), std::any::type_name::<T>());

    for entity in entities_to_init {
        // Get the component state
        let Some(state) = world.get::<T>(entity) else {
            warn!("Failed to get component state for entity {:?}", entity);
            continue
        };

        info!("Rendering component for entity {:?}", entity);

        // Create UiContext and call render function
        let mut ctx = UiContext::new(state, entity, world);
        let html = T::render(&mut ctx);

        info!("Rendered HTML (length: {})", html.content.len());

        // Extract callbacks from context
        let callbacks = ctx.take_callbacks();

        info!("Extracted {} callbacks", callbacks.len());

        // Insert ValorUi component
        world.entity_mut(entity).insert(ValorUi::new(html.content.clone()).with_width(800).with_height(600));

        // Store callbacks for event handling
        if !callbacks.is_empty() {
            world.entity_mut(entity).insert(super::context::ReactiveCallbacks::new(callbacks));
        }

        info!("✅ Initialized reactive component for entity {:?}", entity);
    }
}

/// Handle click events for reactive components
fn handle_click_events<T: Component>(
    trigger: Trigger<OnClick>,
    handlers_query: Query<&ClickHandler>,
    mut commands: Commands,
) {
    // Get the handler that was clicked
    let Ok(handler) = handlers_query.get(trigger.entity()) else { return };
    let handler_name = handler.name.clone();

    info!("Click event on handler: {}", handler_name);

    // Queue callback execution
    commands.queue(move |world: &mut World| {
        execute_callback::<T>(world, &handler_name);
    });
}

/// Detect changes in component state and trigger re-renders
fn detect_changes<T: Component>(
    changed: Query<(Entity, &T), Changed<T>>,
    mut commands: Commands,
) {
    for (entity, _state) in &changed {
        // Queue a re-render
        commands.queue(move |world: &mut World| {
            rerender_reactive_component::<T>(world, entity);
        });
    }
}

/// Re-render a reactive component after state change
fn rerender_reactive_component<T: Component>(world: &mut World, entity: Entity) {
    // Get the component state
    let Some(state) = world.get::<T>(entity) else { return };

    // Create UiContext and call render function
    let mut ctx = UiContext::new(state, entity, world);
    let html = T::render(&mut ctx);

    // Extract callbacks from context
    let callbacks = ctx.take_callbacks();

    // Update the ValorUi component
    if let Some(mut valor_ui) = world.get_mut::<ValorUi>(entity) {
        valor_ui.html = html.content;

        // Update callbacks
        if !callbacks.is_empty() {
            world.entity_mut(entity).insert(super::context::ReactiveCallbacks::new(callbacks));
        }

        // Trigger a re-render in the valor integration
        crate::bevy_integration::rerender_valor_ui(world, entity);
    }
}

/// Register click handlers from reactive callbacks
fn register_click_handlers<T: Component>(
    mut commands: Commands,
    query: Query<(Entity, &super::context::ReactiveCallbacks<T>), Added<super::context::ReactiveCallbacks<T>>>,
) {
    for (entity, callbacks) in &query {
        // Register a ClickHandler for each callback name
        for name in callbacks.handlers.keys() {
            info!("Registering click handler '{}' for reactive component entity {:?}", name, entity);
            commands.spawn(ClickHandler {
                name: name.clone(),
            });
        }
    }
}

/// Execute a callback by name on all components with that callback registered
fn execute_callback<T: Component>(world: &mut World, callback_name: &str) {
    // Find all entities with T component and ReactiveCallbacks
    let entities_with_callbacks: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, (With<T>, With<super::context::ReactiveCallbacks<T>>)>();
        query.iter(world).collect()
    };

    info!("execute_callback: Found {} entities with callbacks for '{}'", entities_with_callbacks.len(), callback_name);

    for entity in entities_with_callbacks {
        // Get the callback function
        let callback_fn = {
            let callbacks = world.get::<super::context::ReactiveCallbacks<T>>(entity);
            callbacks.and_then(|cb| cb.get(callback_name).cloned())
        };

        if let Some(callback) = callback_fn {
            info!("Executing callback '{}' on entity {:?}", callback_name, entity);

            // Execute the callback with mutable access to the component
            if let Some(mut component) = world.get_mut::<T>(entity) {
                info!("Got mutable component, calling callback...");
                callback(&mut *component);
                info!("Callback executed, marking as changed");
                component.set_changed();
            } else {
                warn!("Failed to get mutable component for entity {:?}", entity);
            }
        } else {
            warn!("No callback found for '{}' on entity {:?}", callback_name, entity);
        }
    }
}
