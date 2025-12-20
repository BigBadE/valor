//! Bevy components and resources for Valor UI integration

use crate::reactive::Html;
use bevy::asset::Handle;
use bevy::prelude::*;
use js::NodeKey;
use page_handler::core::state::HtmlPage;
use std::collections::HashMap;
use tokio::runtime::Handle as TokioRuntimeHandle;

/// Component marking an entity as hosting a Valor UI
#[derive(Component)]
pub struct ValorUi {
    /// Width of the UI viewport
    pub width: u32,
    /// Height of the UI viewport
    pub height: u32,
    /// HTML content to render (as DOMUpdates)
    pub html: Html,
    /// Track if this is the first render
    pub first_render: bool,
}

impl ValorUi {
    /// Create a new Valor UI with the given dimensions and HTML
    pub fn new(html: Html) -> Self {
        Self {
            width: 1024,
            height: 768,
            html,
            first_render: true,
        }
    }

    /// Set the viewport width
    pub fn with_width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    /// Set the viewport height
    pub fn with_height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }
}

/// Marker component indicating this ValorUi has been initialized
#[derive(Component)]
pub struct ValorPageInitialized;

/// Component holding the rendered texture and display node for a ValorUi
#[derive(Component)]
pub struct ValorTexture {
    pub image_handle: Handle<Image>,
    pub display_node: Entity,
}

/// Persistent GPU rendering context to avoid recreating GPU resources on every render
pub struct PersistentRenderContext {
    /// Width of the rendering viewport
    pub width: u32,
    /// Height of the rendering viewport
    pub height: u32,
    /// Reusable GPU context (device and queue)
    pub gpu_context: wgpu_backend::offscreen::PersistentGpuContext,
}

/// Non-send resource holding all active HtmlPage instances
/// HtmlPage contains V8 which is !Send, so we need to keep it in a non-send resource
#[derive(Default)]
pub struct ValorPages {
    /// Map from Bevy entity ID to HtmlPage instance
    pub pages: HashMap<Entity, HtmlPage>,
    /// Map from (page entity, NodeKey) to handler name for onclick attributes
    pub click_handlers: HashMap<(Entity, NodeKey), String>,
    /// Map from entity ID to persistent render context for GPU reuse
    pub render_contexts: HashMap<Entity, PersistentRenderContext>,
    /// Map from image URLs to Bevy image handles
    pub image_assets: HashMap<String, Handle<Image>>,
}

/// Component marking an entity as a handler target for a named event
/// The onclick="handler_name" in HTML will trigger events on entities with this component
#[derive(Component)]
pub struct ClickHandler {
    /// The handler name from HTML (e.g., "increment_counter")
    pub name: String,
}

/// Resource holding the Tokio runtime handle for async operations
#[derive(Resource)]
pub struct TokioHandle(pub TokioRuntimeHandle);

/// Resource for managing image assets loaded from file paths or URLs
#[derive(Resource, Default)]
pub struct ImageRegistry {
    /// Map from image source (file path or URL) to Bevy Handle<Image>
    pub images: HashMap<String, Handle<Image>>,
    /// Pending image loads (source -> entity requesting it)
    pub pending: HashMap<String, Vec<Entity>>,
}

impl ImageRegistry {
    /// Register an image source for loading
    pub fn register_image(&mut self, source: impl Into<String>, entity: Entity) {
        let source = source.into();
        self.pending.entry(source).or_default().push(entity);
    }

    /// Get an already-loaded image handle
    pub fn get_image(&self, source: &str) -> Option<Handle<Image>> {
        self.images.get(source).cloned()
    }

    /// Mark an image as loaded
    pub fn set_loaded(&mut self, source: impl Into<String>, handle: Handle<Image>) {
        let source = source.into();
        self.images.insert(source.clone(), handle);
        self.pending.remove(&source);
    }
}

/// Component for requesting an image to be loaded via Bevy's asset system
#[derive(Component)]
pub struct ImageAssetRequest {
    /// The image source (file path or URL)
    pub source: String,
}

/// Global styles resource for ValorUi (theme + Tailwind utilities)
#[derive(Resource, Clone)]
pub struct GlobalStyles(pub String);

/// Marker component indicating click handlers have been extracted
#[derive(Component)]
pub(super) struct ClickHandlersExtracted;
