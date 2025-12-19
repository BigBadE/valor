//! Bevy ECS integration for Valor DSL
//!
//! This module provides components, resources, and systems for rendering
//! HTML/CSS UIs within Bevy applications using the Valor browser engine.

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use page_handler::core::state::HtmlPage;
use std::collections::HashMap;
use tokio::runtime::Handle as TokioRuntimeHandle;
use url::Url;

/// Component marking an entity as hosting a Valor UI
#[derive(Component)]
pub struct ValorUi {
    /// Width of the UI viewport
    pub width: u32,
    /// Height of the UI viewport
    pub height: u32,
    /// HTML content to render
    pub html: String,
}

/// Marker component indicating this ValorUi has been initialized
#[derive(Component)]
pub struct ValorPageInitialized;

/// Component holding the rendered texture and display node for a ValorUi
#[derive(Component)]
pub struct ValorTexture {
    pub image_handle: bevy::asset::Handle<Image>,
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
    pub click_handlers: HashMap<(Entity, js::NodeKey), String>,
    /// Map from entity ID to persistent render context for GPU reuse
    pub render_contexts: HashMap<Entity, PersistentRenderContext>,
    /// Map from image URLs to Bevy image handles
    pub image_assets: HashMap<String, Handle<Image>>,
}

impl ValorUi {
    /// Create a new Valor UI with the given dimensions and HTML
    pub fn new(html: impl Into<String>) -> Self {
        Self {
            width: 1024,
            height: 768,
            html: html.into(),
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

/// Component marking an entity as a handler target for a named event
/// The onclick="handler_name" in HTML will trigger events on entities with this component
#[derive(Component)]
pub struct ClickHandler {
    /// The handler name from HTML (e.g., "increment_counter")
    pub name: String,
}

/// Plugin to add Valor UI support to Bevy applications
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

pub struct ValorUiPlugin;

impl Plugin for ValorUiPlugin {
    fn build(&self, app: &mut App) {
        // Create a Tokio runtime for async HtmlPage operations
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        let handle = runtime.handle().clone();

        app.insert_non_send_resource(runtime)
            .insert_non_send_resource(ValorPages::default())
            .insert_resource(TokioHandle(handle))
            .insert_resource(ImageRegistry::default())
            .add_event::<crate::bevy_events::OnClick>()
            .add_event::<crate::bevy_events::OnInput>()
            .add_event::<crate::bevy_events::OnChange>()
            .add_event::<crate::bevy_events::OnSubmit>()
            .add_event::<crate::bevy_events::OnFocus>()
            .add_event::<crate::bevy_events::OnBlur>()
            .add_event::<crate::bevy_events::OnKeyDown>()
            .add_event::<crate::bevy_events::OnKeyUp>()
            .add_event::<crate::bevy_events::OnMouseEnter>()
            .add_event::<crate::bevy_events::OnMouseLeave>()
            .add_event::<crate::bevy_events::OnMouseMove>()
            .add_systems(
                Update,
                (
                    process_new_valor_uis,
                    update_valor_pages,
                    extract_click_handlers,
                    render_valor_pages,
                    handle_mouse_clicks,
                    handle_window_resize,
                    test_any_input,
                    load_image_assets,
                )
                    .chain(),
            );

        info!("Valor UI Plugin initialized");
    }
}

/// System that processes newly added ValorUi components and creates HtmlPage instances
fn process_new_valor_uis(
    mut commands: Commands,
    query: Query<(Entity, &ValorUi), (Added<ValorUi>, Without<ValorPageInitialized>)>,
    tokio_handle: Res<TokioHandle>,
    mut pages: NonSendMut<ValorPages>,
) {
    for (entity, valor_ui) in &query {
        info!("Processing new ValorUi entity: {:?}", entity);

        // Write HTML to a temporary file and create a file:// URL
        let temp_file_path = std::env::temp_dir().join(format!("valor_ui_{:?}.html", entity));
        if let Err(err) = std::fs::write(&temp_file_path, &valor_ui.html) {
            error!("Failed to write HTML to temp file: {}", err);
            continue;
        }

        let url = match Url::from_file_path(&temp_file_path) {
            Ok(u) => u,
            Err(()) => {
                error!("Failed to create file URL from path: {:?}", temp_file_path);
                continue;
            }
        };

        // Create HtmlPage asynchronously
        let handle = tokio_handle.0.clone();
        let width = valor_ui.width;
        let height = valor_ui.height;

        let page_result = handle.block_on(async {
            let config = page_handler::utilities::config::ValorConfig::from_env();
            HtmlPage::new(&handle, url, config).await
        });

        match page_result {
            Ok(mut page) => {
                // Set viewport dimensions
                page.set_viewport(width as i32, height as i32);

                // Store the page in the non-send resource
                pages.pages.insert(entity, page);

                // Mark this entity as initialized
                commands.entity(entity).insert(ValorPageInitialized);
                info!("Successfully created HtmlPage for entity {:?}", entity);
            }
            Err(err) => {
                error!("Failed to create HtmlPage: {}", err);
            }
        }
    }
}

/// System that extracts onclick handlers from newly initialized pages
fn extract_click_handlers(
    query: Query<Entity, (With<ValorPageInitialized>, Without<ClickHandlersExtracted>)>,
    mut commands: Commands,
    mut pages: NonSendMut<ValorPages>,
) {
    for entity in &query {
        if let Some(page) = pages.pages.get_mut(&entity) {
            info!("Extracting click handlers for entity {:?}", entity);

            // Get the attributes map from the layout
            let attrs_map = page.layouter_attrs_map();

            // Scan for onclick attributes
            for (node_key, attrs) in &attrs_map {
                if let Some(handler_name) = attrs.get("onclick") {
                    info!(
                        "Found onclick handler '{}' on node {:?}",
                        handler_name, node_key
                    );
                    pages
                        .click_handlers
                        .insert((entity, *node_key), handler_name.clone());
                }
            }

            let handler_count = pages.click_handlers.iter().filter(|((e, _), _)| *e == entity).count();
            info!("Extracted {} click handlers for entity {:?}", handler_count, entity);

            // Mark this entity as having handlers extracted
            commands.entity(entity).insert(ClickHandlersExtracted);
        }
    }
}

/// Marker component indicating click handlers have been extracted
#[derive(Component)]
struct ClickHandlersExtracted;

/// System that updates all active Valor pages
fn update_valor_pages(mut pages: NonSendMut<ValorPages>, tokio_handle: Res<TokioHandle>) {
    for page in pages.pages.values_mut() {
        let handle = tokio_handle.0.clone();

        // Run the page update
        let update_result = handle.block_on(async { page.update().await });

        if let Err(err) = update_result {
            error!("Failed to update HtmlPage: {}", err);
        }
    }
}

/// System that renders Valor pages to textures
fn render_valor_pages(
    mut commands: Commands,
    query: Query<(Entity, &ValorUi), (With<ValorPageInitialized>, Without<ValorTexture>)>,
    mut pages: NonSendMut<ValorPages>,
    mut images: ResMut<Assets<Image>>,
) {
    for (entity, valor_ui) in &query {
        if let Some(page) = pages.pages.get_mut(&entity) {
            // Get the display list from the page
            let display_list = page.display_list_retained_snapshot();

            info!("Rendering ValorUi with {} display items", display_list.items.len());

            // Create a simple texture (we'll render to this later)
            // For now, create a placeholder white texture
            let width = valor_ui.width;
            let height = valor_ui.height;

            // Render the display list to pixels using wgpu_backend
            let image_data = match wgpu_backend::render_display_list_to_rgba(&display_list, width, height) {
                Ok(data) => data,
                Err(err) => {
                    error!("Failed to render display list: {}", err);
                    // Fall back to placeholder light gray
                    let mut fallback = vec![255u8; (width * height * 4) as usize];
                    for pixel in fallback.chunks_exact_mut(4) {
                        pixel[0] = 240; // R
                        pixel[1] = 240; // G
                        pixel[2] = 245; // B
                        pixel[3] = 255; // A
                    }
                    fallback
                }
            };

            let image = Image::new(
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                image_data,
                TextureFormat::Rgba8UnormSrgb,
                Default::default(),
            );

            let image_handle = images.add(image);

            // Spawn a full-screen UI node with the rendered texture
            // IMPORTANT: Don't add Interaction or any picking components - we want window-level mouse events
            let display_node = commands.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                ImageNode::new(image_handle.clone()),
                bevy::ui::FocusPolicy::Pass, // Allow mouse events to pass through to window
            )).id();

            // Add the texture component to track it
            commands.entity(entity).insert(ValorTexture {
                image_handle,
                display_node,
            });

            info!("Created texture for ValorUi entity {:?} (size: {}x{}) with display node {:?}", entity, width, height, display_node);
        }
    }
}


/// Command to trigger a re-render of a ValorUi entity
pub fn rerender_valor_ui(
    world: &mut World,
    valor_ui_entity: Entity,
) {
    // Get the components we need
    let (width, height, display_node, html) = {
        let valor_ui = world.get::<ValorUi>(valor_ui_entity);
        let Some(ui) = valor_ui else { return };
        let texture = world.get::<ValorTexture>(valor_ui_entity);
        let Some(tex) = texture else { return };
        (ui.width, ui.height, tex.display_node, ui.html.clone())
    };

    // Get tokio handle before borrowing pages mutably
    let tokio_handle = world.get_resource::<TokioHandle>().unwrap().0.clone();

    // Get or create persistent render context
    let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

    // Initialize persistent context if not present
    if !pages.render_contexts.contains_key(&valor_ui_entity) {
        match wgpu_backend::initialize_persistent_context(width, height) {
            Ok(ctx) => {
                pages.render_contexts.insert(valor_ui_entity, PersistentRenderContext {
                    width,
                    height,
                    gpu_context: ctx,
                });
                info!("Initialized persistent GPU context for entity {:?}", valor_ui_entity);
            }
            Err(err) => {
                error!("Failed to initialize persistent GPU context: {}", err);
                return;
            }
        }
    }

    // For reactive components, we need to re-parse and update the entire page
    // Write the new HTML to a temp file and reload the page

    info!("🔄 Re-rendering reactive component (HTML length: {})", html.len());

    // Write HTML to a temporary file
    let temp_file_path = std::env::temp_dir().join(format!("valor_ui_rerender_{:?}.html", valor_ui_entity));
    if let Err(err) = std::fs::write(&temp_file_path, &html) {
        error!("Failed to write HTML to temp file: {}", err);
        return;
    }

    info!("🔄 Wrote HTML to temp file: {:?}", temp_file_path);

    // Drop the old page and create a new one with the updated HTML
    pages.pages.remove(&valor_ui_entity);

    let url = match Url::from_file_path(&temp_file_path) {
        Ok(u) => u,
        Err(()) => {
            error!("Failed to create file URL from path: {:?}", temp_file_path);
            return;
        }
    };

    // Create a new HtmlPage with the updated HTML
    let page_result = tokio_handle.block_on(async {
        let config = page_handler::utilities::config::ValorConfig::from_env();
        HtmlPage::new(&tokio_handle, url, config).await
    });

    let mut new_page = match page_result {
        Ok(page) => page,
        Err(err) => {
            error!("Failed to create new HtmlPage: {}", err);
            return;
        }
    };

    // Set viewport dimensions
    new_page.set_viewport(width as i32, height as i32);

    // Update the page
    if let Err(err) = tokio_handle.block_on(async { new_page.update().await }) {
        error!("Failed to update new HtmlPage: {}", err);
        return;
    }

    // Store the new page
    let page = pages.pages.entry(valor_ui_entity).or_insert(new_page);

    let display_list = page.display_list_retained_snapshot();

    // Get mutable reference to render context
    let Some(render_ctx) = pages.render_contexts.get_mut(&valor_ui_entity) else { return };

    // Render to pixels using persistent context
    let image_data = match wgpu_backend::render_display_list_with_context(
        &mut render_ctx.gpu_context,
        &display_list,
        width,
        height,
    ) {
        Ok(data) => data,
        Err(err) => {
            error!("Failed to render display list: {}", err);
            return;
        }
    };

    // Drop the ValorPages borrow before accessing Assets
    drop(pages);

    // Create a new Image with the updated data
    let new_image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        image_data,
        TextureFormat::Rgba8UnormSrgb,
        Default::default(),
    );

    // Add the new image to assets
    let mut images = world.get_resource_mut::<Assets<Image>>().unwrap();
    let new_handle = images.add(new_image);

    // Update the ValorTexture component to track the new handle
    if let Some(mut texture) = world.get_mut::<ValorTexture>(valor_ui_entity) {
        texture.image_handle = new_handle.clone();
    }

    // Update the ImageNode component on the display node to use the new handle
    if let Some(mut image_node) = world.get_mut::<ImageNode>(display_node) {
        *image_node = ImageNode::new(new_handle);
        info!("✅ Re-rendered ValorUi entity {:?} and updated display node {:?}", valor_ui_entity, display_node);
    } else {
        warn!("Failed to get ImageNode for display node {:?}", display_node);
    }

    // Re-extract click handlers after re-render since NodeKeys may have changed
    {
        let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

        // First, get the attrs map
        let attrs_map = if let Some(page) = pages.pages.get_mut(&valor_ui_entity) {
            page.layouter_attrs_map()
        } else {
            return;
        };

        // Clear existing handlers for this entity
        pages.click_handlers.retain(|(e, _), _| *e != valor_ui_entity);

        // Scan for onclick attributes
        for (node_key, attrs) in &attrs_map {
            if let Some(handler_name) = attrs.get("onclick") {
                info!("🔄 Re-extracted onclick handler '{}' on node {:?}", handler_name, node_key);
                pages
                    .click_handlers
                    .insert((valor_ui_entity, *node_key), handler_name.clone());
            }
        }
    }
}

/// Public API: Update the text content of an element by ID
/// This provides two-way data binding: Bevy state → HTML
/// This uses direct DOM manipulation without JavaScript for instant updates
pub fn update_element_text(
    world: &mut World,
    valor_ui_entity: Entity,
    element_id: &str,
    text: &str,
) {
    // Get the TokioHandle first (before any mutable borrows)
    let tokio_handle = world.get_resource::<TokioHandle>().unwrap();
    let handle = tokio_handle.0.clone();

    {
        let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

        if let Some(page) = pages.pages.get_mut(&valor_ui_entity) {
            // Get the NodeKey for the element
            let node_key = match page.get_element_by_id(element_id) {
                Some(key) => key,
                None => {
                    error!("Element with id '{}' not found", element_id);
                    return;
                }
            };

            // Get the first child (text node) of the element
            let text_node_key = {
                let dom_index_shared = page.dom_index_shared();
                let guard = dom_index_shared.lock().unwrap();
                let children = guard.children_by_parent.get(&node_key);
                match children.and_then(|c| c.first().copied()) {
                    Some(child_key) => child_key,
                    None => {
                        error!("Element '{}' has no text node child", element_id);
                        return;
                    }
                }
            };

            // Use the new UpdateText variant for in-place update (no node recreation)
            let updates = vec![js::DOMUpdate::UpdateText {
                node: text_node_key,
                text: text.to_string(),
            }];

            if let Err(err) = page.send_dom_updates(updates) {
                error!("Failed to send DOM updates: {}", err);
                return;
            }

            // Apply the DOM updates immediately before re-rendering
            if let Err(err) = handle.block_on(async { page.update().await }) {
                error!("Failed to apply DOM updates: {}", err);
                return;
            }

            info!("Updated element '{}' text to: {}", element_id, text);
        } else {
            return;
        }
    }

    // Trigger a re-render now that the DOM has changed
    rerender_valor_ui(world, valor_ui_entity);
}

/// Public API: Get element text content by ID
pub fn get_element_text(
    world: &mut World,
    valor_ui_entity: Entity,
    element_id: &str,
) -> Option<String> {
    let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

    pages
        .pages
        .get_mut(&valor_ui_entity)
        .and_then(|page| page.text_content_by_id_sync(element_id))
}

/// Find the handler name for a click at the given coordinates
fn find_click_handler(
    pages: &mut ValorPages,
    valor_ui_entity: Entity,
    x: f32,
    y: f32,
) -> Option<String> {
    let page = pages.pages.get_mut(&valor_ui_entity)?;
    let geometry = page.layouter_geometry_mut();

    // Simple hit test: find the first node containing the click point
    // TODO: This should do proper hit testing with z-order
    for (node_key, rect) in geometry {
        let contains_point = x >= rect.x
            && x <= rect.x + rect.width
            && y >= rect.y
            && y <= rect.y + rect.height;

        if contains_point
            && let Some(handler_name) = pages.click_handlers.get(&(valor_ui_entity, node_key))
        {
            return Some(handler_name.clone());
        }
    }
    None
}

/// Public API: Dispatch a click event at the given coordinates for a ValorUi entity
/// This should be called from input handling code (e.g., winit mouse events)
pub fn dispatch_click(world: &mut World, valor_ui_entity: Entity, x: f32, y: f32, button: u8) {
    // First, find which handler was clicked (if any)
    let handler_name_opt = {
        let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();
        find_click_handler(&mut pages, valor_ui_entity, x, y)
    };

    // Then, find all entities with ClickHandler component matching this name and trigger events
    if let Some(handler_name) = handler_name_opt {
        let handler_entities: Vec<Entity> = world
            .query_filtered::<Entity, With<ClickHandler>>()
            .iter(world)
            .filter(|&entity| {
                world
                    .get::<ClickHandler>(entity)
                    .is_some_and(|h| h.name == handler_name)
            })
            .collect();

        let event = crate::bevy_events::OnClick {
            node: js::NodeKey::ROOT, // TODO: Use actual NodeKey
            position: (x, y),
            button,
        };

        for handler_entity in handler_entities {
            info!(
                "Dispatching click event to handler '{}' (entity {:?})",
                handler_name, handler_entity
            );
            world.trigger_targets(event.clone(), handler_entity);
        }
    }
}

/// Debug system to test if ANY input is being received
fn test_any_input(
    keys: Res<bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>>,
    mouse: Res<bevy::input::ButtonInput<bevy::input::mouse::MouseButton>>,
    windows: Query<&bevy::window::Window>,
) {
    use bevy::input::keyboard::KeyCode;
    use bevy::input::mouse::MouseButton;

    if keys.get_just_pressed().len() > 0 {
        info!("⌨️ Key press detected!");
    }

    if mouse.get_just_pressed().len() > 0 {
        info!("🖱️ Raw mouse button press detected in test_any_input!");
        if let Ok(window) = windows.get_single() {
            if let Some(pos) = window.cursor_position() {
                info!("🖱️ Cursor position: ({}, {})", pos.x, pos.y);
            } else {
                warn!("🖱️ Cursor position is None!");
            }
        }
    }
}

/// System that handles mouse button input and dispatches clicks to Valor UIs
fn handle_mouse_clicks(
    mouse_button_input: Res<bevy::input::ButtonInput<bevy::input::mouse::MouseButton>>,
    windows: Query<&bevy::window::Window>,
    valor_uis: Query<Entity, With<ValorUi>>,
    mut commands: Commands,
) {
    use bevy::input::mouse::MouseButton;

    // Check if left mouse button was just pressed
    if mouse_button_input.just_pressed(MouseButton::Left) {
        info!("🖱️ Mouse click detected!");
        // Get the primary window's cursor position
        if let Ok(window) = windows.get_single() {
            if let Some(cursor_pos) = window.cursor_position() {
                // Convert from window coordinates (origin top-left) to Valor coordinates
                let x = cursor_pos.x;
                let y = cursor_pos.y;

                info!("🖱️ Click at position: ({}, {})", x, y);

                // Dispatch click to all Valor UI entities
                // In a real app, you'd do hit testing to find which UI was clicked
                for valor_ui_entity in &valor_uis {
                    info!("🖱️ Dispatching click to ValorUi entity {:?}", valor_ui_entity);
                    let button = 0; // Left button
                    commands.queue(move |world: &mut World| {
                        dispatch_click(world, valor_ui_entity, x, y, button);
                    });
                }
            } else {
                warn!("🖱️ Click detected but no cursor position");
            }
        } else {
            warn!("🖱️ Click detected but no window found");
        }
    }
}

/// System that handles window resize events and updates Valor page viewports
fn handle_window_resize(
    mut commands: Commands,
    mut valor_query: Query<(&mut ValorUi, Entity), With<ValorPageInitialized>>,
    windows: Query<&Window, Changed<Window>>,
) {
    for window in &windows {
        let width = window.width() as u32;
        let height = window.height() as u32;

        for (mut valor_ui, entity) in &mut valor_query {
            if valor_ui.width != width || valor_ui.height != height {
                info!("🪟 Window resized: {}x{} -> {}x{}", valor_ui.width, valor_ui.height, width, height);

                // Update ValorUi dimensions
                valor_ui.width = width;
                valor_ui.height = height;

                // Queue a viewport update and re-render
                commands.queue(move |world: &mut World| {
                    // Get the tokio handle
                    let tokio_handle = world.get_resource::<TokioHandle>().unwrap().0.clone();
                    let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

                    // Update the HtmlPage viewport
                    if let Some(page) = pages.pages.get_mut(&entity) {
                        page.set_viewport(width as i32, height as i32);

                        // Recompute layout with new viewport
                        let update_result = tokio_handle.block_on(async { page.update().await });
                        if let Err(err) = update_result {
                            error!("Failed to update page after resize: {}", err);
                            return;
                        }

                        info!("✅ Updated viewport and recomputed layout for entity {:?}", entity);

                        // Get the new display list
                        let display_list = page.display_list_retained_snapshot();

                        // Don't recreate GPU context - just render with new dimensions
                        // The render function will handle the resize internally
                        match wgpu_backend::render_display_list_to_rgba(&display_list, width, height) {
                            Ok(image_data) => {
                                // Update the Image asset with new dimensions and data
                                let image_handle = {
                                    let Some(texture) = world.get::<ValorTexture>(entity) else {
                                        warn!("No ValorTexture for entity {:?}", entity);
                                        return;
                                    };
                                    texture.image_handle.clone()
                                };

                                if let Some(mut images) = world.get_resource_mut::<Assets<Image>>() {
                                    if let Some(image) = images.get_mut(&image_handle) {
                                        // Update texture size and data
                                        image.resize(bevy::render::render_resource::Extent3d {
                                            width,
                                            height,
                                            depth_or_array_layers: 1,
                                        });
                                        image.data = image_data;
                                        info!("✅ Updated texture after window resize to {}x{}", width, height);
                                    }
                                }
                            }
                            Err(err) => {
                                error!("Failed to render after resize: {}", err);
                            }
                        }
                    }
                });
            }
        }
    }
}

/// System that loads image assets requested via ImageAssetRequest components
fn load_image_assets(
    mut commands: Commands,
    requests: Query<(Entity, &ImageAssetRequest), Added<ImageAssetRequest>>,
    asset_server: Res<AssetServer>,
    mut registry: ResMut<ImageRegistry>,
) {
    for (entity, request) in &requests {
        let source = &request.source;

        // Check if already loaded
        if registry.get_image(source).is_some() {
            info!("Image already loaded: {}", source);
            commands.entity(entity).remove::<ImageAssetRequest>();
            continue;
        }

        // Check if already pending
        if registry.pending.contains_key(source) {
            info!("Image already pending: {}", source);
            registry.register_image(source, entity);
            commands.entity(entity).remove::<ImageAssetRequest>();
            continue;
        }

        // Load the image via Bevy's asset server
        info!("Loading image asset: {}", source);
        let handle: Handle<Image> = asset_server.load(source.clone());

        // Register it
        registry.set_loaded(source, handle);
        commands.entity(entity).remove::<ImageAssetRequest>();
    }
}

/// Public API: Load an image into the Valor UI system
/// Returns the image handle that can be used in Bevy UI or passed to Valor
pub fn load_image(
    commands: &mut Commands,
    source: impl Into<String>,
) -> Entity {
    let source = source.into();
    commands.spawn(ImageAssetRequest { source }).id()
}

/// Public API: Get an image handle by source path
pub fn get_image_handle(
    registry: &ImageRegistry,
    source: &str,
) -> Option<Handle<Image>> {
    registry.get_image(source)
}
