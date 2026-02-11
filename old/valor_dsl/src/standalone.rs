//! Standalone integration for Valor without full Bevy App
//!
//! This module allows Valor to work with just bevy_ecs and custom schedules,
//! without requiring Bevy's full App and plugin system.

#[cfg(feature = "bevy_integration")]
use crate::bevy_integration::*;
use crate::reactive::{Component, ComponentFn, UiContext};
use crate::styling::{TailwindUtilities, Theme};
use bevy::ecs::component::Mutable;
use bevy::ecs::entity::Entity;
use bevy::ecs::schedule::Schedule;
use bevy::ecs::world::World;
use log::{info, warn};
use std::any::TypeId;
use std::collections::HashMap;

use bevy::ecs::prelude::Resource;

/// Global styles resource (theme + Tailwind utilities)
#[derive(Resource, Clone)]
pub struct GlobalStyles(pub String);

/// Registry of component render functions
#[derive(Resource, Default)]
pub struct ComponentRegistry {
    renderers: HashMap<TypeId, Box<dyn Fn(Entity, &World) -> String + Send + Sync>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: Component<Mutability = Mutable>>(&mut self, _render_fn: ComponentFn<T>) {
        // Store type info for this component
        self.renderers
            .insert(TypeId::of::<T>(), Box::new(|_entity, _world| String::new()));
    }
}

/// Initialize Valor resources in a World
///
/// Call this once during startup to set up Valor resources.
pub fn initialize_valor_resources(world: &mut World) {
    info!("Initializing Valor standalone resources");

    // Generate global CSS
    let theme = Theme::default();
    let global_css = format!(
        "{}\n{}",
        theme.to_css(),
        TailwindUtilities::generate(&theme.colors)
    );

    // Insert resources
    world.insert_resource(GlobalStyles(global_css));
    world.insert_resource(ComponentRegistry::new());
    world.insert_resource(ImageRegistry::default());

    // Initialize Bevy Assets resource for image management
    world.insert_resource(bevy::asset::Assets::<bevy::prelude::Image>::default());

    // Create Tokio runtime for async operations
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let handle = runtime.handle().clone();

    world.insert_non_send_resource(runtime);
    world.insert_non_send_resource(ValorPages::default());
    world.insert_resource(TokioHandle(handle));

    info!("Valor standalone resources initialized");
}

/// Register a reactive component type
///
/// This should be called for each component type during startup.
pub fn register_reactive_component<T: Component<Mutability = Mutable>>(
    world: &mut World,
    _render_fn: ComponentFn<T>,
) {
    if let Some(mut registry) = world.get_resource_mut::<ComponentRegistry>() {
        registry.register::<T>(_render_fn);
    }
}

/// Initialize a component by rendering it and setting up UI
///
/// This should be called in an Update schedule.
pub fn initialize_component<T: Component>(world: &mut World) {
    // Collect entities to initialize
    let entities_to_init: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, (
            bevy::ecs::query::With<T>,
            bevy::ecs::query::Without<ValorUi>,
        )>();
        query.iter(world).collect()
    };

    if entities_to_init.is_empty() {
        return;
    }

    info!(
        "initialize_component: Found {} entities to initialize for type {}",
        entities_to_init.len(),
        std::any::type_name::<T>()
    );

    // Get global styles
    let global_styles = world
        .get_resource::<GlobalStyles>()
        .map(|s| s.0.clone())
        .unwrap_or_default();

    for entity in entities_to_init {
        // Get the component state
        let Some(state) = world.get::<T>(entity) else {
            warn!("Failed to get component state for entity {:?}", entity);
            continue;
        };

        info!("Rendering component for entity {:?}", entity);

        // Create UiContext and call render function
        let mut ctx = UiContext::new(state, entity, world);
        let mut html = T::render(&mut ctx);

        info!("Rendered HTML ({} DOM updates)", html.updates.len());

        // Prepend global styles
        html.prepend_global_styles(&global_styles);

        // Extract callbacks from context
        let callbacks = ctx.take_callbacks();

        info!("Extracted {} callbacks", callbacks.len());

        // Insert ValorUi component with the rendered HTML
        // Default to 800x600, but this will be updated by handle_window_resize
        world
            .entity_mut(entity)
            .insert(ValorUi::new(html).with_width(800).with_height(600));

        // Store callbacks for event handling
        if !callbacks.is_empty() {
            world
                .entity_mut(entity)
                .insert(crate::reactive::context::ReactiveCallbacks::<T>::new(
                    callbacks,
                ));
        }

        info!("✅ Initialized reactive component for entity {:?}", entity);
    }
}

/// Detect changes in components and trigger re-renders
pub fn detect_changes<T: Component>(world: &mut World) {
    // Get entities that have both T and ValorUi, and T has changed
    let changed_entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, (
            bevy::ecs::query::With<T>,
            bevy::ecs::query::With<ValorUi>,
            bevy::ecs::query::Changed<T>,
        )>();
        query.iter(world).collect()
    };

    if changed_entities.is_empty() {
        return;
    }

    info!(
        "detect_changes: {} entities changed for type {}",
        changed_entities.len(),
        std::any::type_name::<T>()
    );

    let global_styles = world
        .get_resource::<GlobalStyles>()
        .map(|s| s.0.clone())
        .unwrap_or_default();

    for entity in changed_entities {
        let Some(state) = world.get::<T>(entity) else {
            continue;
        };

        info!("Re-rendering component for entity {:?}", entity);

        let mut ctx = UiContext::new(state, entity, world);
        let mut html = T::render(&mut ctx);
        html.prepend_global_styles(&global_styles);

        let callbacks = ctx.take_callbacks();

        // Update ValorUi component - preserve existing dimensions
        if let Some(mut valor_ui) = world.get_mut::<ValorUi>(entity) {
            let width = valor_ui.width;
            let height = valor_ui.height;
            *valor_ui = ValorUi::new(html).with_width(width).with_height(height);
        }

        // Update callbacks
        if !callbacks.is_empty() {
            world
                .entity_mut(entity)
                .insert(crate::reactive::context::ReactiveCallbacks::<T>::new(
                    callbacks,
                ));
        }
    }
}

/// Register click handlers from reactive callbacks
pub fn register_click_handlers<T: Component>(world: &mut World) {
    let entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, (
            bevy::ecs::query::With<T>,
            bevy::ecs::query::With<crate::reactive::context::ReactiveCallbacks<T>>,
        )>();
        query.iter(world).collect()
    };

    for entity in entities {
        // Implementation would extract click handlers and register them
        // This is a simplified version
        let _ = entity;
    }
}

/// Add all Valor systems to a schedule
///
/// Call this to add Valor's update systems to your Update schedule.
#[cfg(feature = "bevy_integration")]
pub fn add_valor_systems_to_schedule(schedule: &mut Schedule) {
    use crate::bevy_integration::systems::*;

    schedule.add_systems((
        process_new_valor_uis,
        update_valor_pages,
        extract_click_handlers,
        render_valor_pages,
        handle_mouse_clicks,
        handle_window_resize,
        test_any_input,
        load_image_assets,
    ));

    info!("Added Valor systems to schedule");
}
