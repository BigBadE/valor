//! Bevy ECS integration for Valor DSL
//!
//! This module provides Bevy components, resources, and systems for rendering
//! HTML/CSS UIs within Bevy applications.

use anyhow::Result;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use js::{DOMUpdate, KeySpace, NodeKey};

use crate::VirtualDom;
use crate::events::EventCallbacks;

/// Component marking an entity as a Valor UI root
#[derive(Component)]
pub struct ValorUiRoot {
    /// Render target dimensions
    pub width: u32,
    pub height: u32,
    /// Flag indicating if the UI needs redraw
    pub dirty: bool,
}

/// Resource for managing Valor UI rendering
#[derive(Resource, Default)]
pub struct ValorRenderer {
    // Placeholder for now - actual rendering integration TBD
}

/// Event triggered when a node is clicked
#[derive(Event)]
pub struct ValorClickEvent {
    pub node: NodeKey,
    pub x: f32,
    pub y: f32,
}

/// Event triggered when text input occurs
#[derive(Event)]
pub struct ValorInputEvent {
    pub node: NodeKey,
    pub value: String,
}

/// Component for the rendered UI texture
#[derive(Component)]
pub struct ValorUiTexture {
    pub texture: Handle<Image>,
}

/// System to initialize Valor UI
pub fn setup_valor_ui(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let width = 1024;
    let height = 768;

    // Create render target texture
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    );
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;

    let texture_handle = images.add(image);

    commands.spawn((
        ValorUiRoot {
            width,
            height,
            dirty: false,
        },
        ValorUiTexture {
            texture: texture_handle.clone(),
        },
        Sprite::from_image(texture_handle),
    ));
}

/// System to update Valor UI
pub fn update_valor_ui(mut query: Query<&mut ValorUiRoot>) {
    for mut ui_root in &mut query {
        if ui_root.dirty {
            // Mark as clean
            ui_root.dirty = false;
        }
    }
}

/// System to handle click events
pub fn handle_click_events(
    mut click_events: EventReader<ValorClickEvent>,
    mut query: Query<&mut ValorUiRoot>,
) {
    for event in click_events.read() {
        for mut ui_root in &mut query {
            // Trigger click handling
            info!(
                "Click event at ({}, {}) on node {:?}",
                event.x, event.y, event.node
            );
            ui_root.dirty = true;
        }
    }
}

/// Helper to create a Valor UI from HTML
///
/// # Errors
/// Returns an error if HTML parsing fails
pub fn create_valor_ui(html: &str, callbacks: &EventCallbacks) -> Result<Vec<DOMUpdate>> {
    // Compile and inject HTML
    let mut vdom = VirtualDom::new({
        let mut keyspace = KeySpace::new();
        keyspace.register_manager()
    });

    vdom.compile_html(html, NodeKey::ROOT, callbacks)
}

/// Plugin to add Valor UI support to Bevy
pub struct ValorUiPlugin;

impl Plugin for ValorUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ValorRenderer>()
            .add_event::<ValorClickEvent>()
            .add_event::<ValorInputEvent>()
            .add_systems(Startup, setup_valor_ui)
            .add_systems(Update, (update_valor_ui, handle_click_events));
    }
}
