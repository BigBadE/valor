//! Bevy systems for Valor UI updates and event handling

use super::components::*;
use super::rendering::render_display_list_to_pixels;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use log::{error, info, warn};
use page_handler::core::state::HtmlPage;

/// System that processes newly added ValorUi components and creates HtmlPage instances
pub fn process_new_valor_uis(
    mut commands: Commands,
    query: Query<(Entity, &ValorUi), (Added<ValorUi>, Without<ValorPageInitialized>)>,
    tokio_handle: Res<TokioHandle>,
    mut pages: NonSendMut<ValorPages>,
) {
    for (entity, valor_ui) in &query {
        info!("Processing new ValorUi entity: {:?}", entity);

        // Note: Reactive components inject global styles in initialize_component before creating ValorUi
        // Observer pattern components should inject styles manually before creating ValorUi
        info!("📋 DOM updates: {}", valor_ui.html.updates.len());

        // Create blank page that will be populated with DOMUpdates
        let handle = tokio_handle.0.clone();
        let (width, height) = (valor_ui.width, valor_ui.height);
        let config = page_handler::utilities::config::ValorConfig::from_env();
        let page_result = HtmlPage::new_blank(&handle, config, true);

        match page_result {
            Ok(mut page) => {
                // Set viewport dimensions
                info!("Setting viewport for {:?} to {}x{}", entity, width, height);
                page.set_viewport(width as i32, height as i32);

                // Let the page process the initial html/body structure first
                if let Err(err) = handle.block_on(async { page.update().await }) {
                    error!("Failed to process initial document structure: {}", err);
                    continue;
                }

                // Find the body element - the blank page creates <html> and <body>
                let body_node = find_body_element(&page);

                let updates_for_body = if let Some(body) = body_node {
                    info!("Found body element: {:?}", body);
                    // Remap ROOT references to body
                    remap_dom_updates_to_body(&valor_ui.html.updates, body)
                } else {
                    // Fallback: use updates as-is if body not found
                    warn!("Body element not found, using ROOT as parent");
                    valor_ui.html.updates.clone()
                };

                // Apply initial DOMUpdates directly
                let update_count = updates_for_body.len();
                info!("📝 Applying {} initial DOM updates", update_count);
                if let Err(err) = page.send_dom_updates(updates_for_body) {
                    error!("Failed to send initial DOMUpdates: {}", err);
                    continue;
                }

                // Update page to compute initial layout
                info!("Computing layout for {} DOM elements", update_count);
                if let Err(err) = handle.block_on(async { page.update().await }) {
                    error!("Failed to update page for initial layout: {}", err);
                    continue;
                }

                // Store page and mark as initialized
                pages.pages.insert(entity, page);
                commands
                    .entity(entity)
                    .insert((ValorPageInitialized, NeedsRender));
                info!("Created HtmlPage for {:?}", entity);
            }
            Err(err) => {
                error!("Failed to create blank HtmlPage: {}", err);
            }
        }
    }
}

/// Find the body element in a page
fn find_body_element(page: &HtmlPage) -> Option<js::NodeKey> {
    let dom_index = page.dom_index_shared();
    let guard = dom_index.lock().unwrap();
    // Find the body element by iterating tag_by_key
    guard
        .tag_by_key
        .iter()
        .find(|(_, tag)| *tag == "body")
        .map(|(node_key, _)| *node_key)
}

/// Remap DOM updates from ROOT to a specific body node
fn remap_dom_updates_to_body(updates: &[js::DOMUpdate], body: js::NodeKey) -> Vec<js::DOMUpdate> {
    updates
        .iter()
        .map(|update| match update {
            js::DOMUpdate::InsertElement {
                parent,
                node,
                tag,
                pos,
            } => {
                let new_parent = if *parent == js::NodeKey::ROOT {
                    body
                } else {
                    *parent
                };
                js::DOMUpdate::InsertElement {
                    parent: new_parent,
                    node: *node,
                    tag: tag.clone(),
                    pos: *pos,
                }
            }
            js::DOMUpdate::InsertText {
                parent,
                node,
                text,
                pos,
            } => {
                let new_parent = if *parent == js::NodeKey::ROOT {
                    body
                } else {
                    *parent
                };
                js::DOMUpdate::InsertText {
                    parent: new_parent,
                    node: *node,
                    text: text.clone(),
                    pos: *pos,
                }
            }
            other => other.clone(),
        })
        .collect()
}

/// System that extracts onclick handlers from newly initialized pages
pub fn extract_click_handlers(
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

            let handler_count = pages
                .click_handlers
                .iter()
                .filter(|((e, _), _)| *e == entity)
                .count();
            info!(
                "Extracted {} click handlers for entity {:?}",
                handler_count, entity
            );

            // Mark this entity as having handlers extracted
            commands.entity(entity).insert(ClickHandlersExtracted);
        }
    }
}

/// System that updates all active Valor pages
pub fn update_valor_pages(mut pages: NonSendMut<ValorPages>, tokio_handle: Res<TokioHandle>) {
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
pub fn render_valor_pages(
    mut commands: Commands,
    query: Query<
        (Entity, &ValorUi, Option<&ValorTexture>),
        (With<ValorPageInitialized>, With<NeedsRender>),
    >,
    mut pages: NonSendMut<ValorPages>,
    mut images: ResMut<Assets<Image>>,
) {
    let entities_to_render: Vec<_> = query.iter().map(|(e, _, _)| e).collect();
    info!(
        "render_valor_pages: Found {} entities with NeedsRender",
        entities_to_render.len()
    );
    for (entity, valor_ui, existing_texture) in &query {
        info!("Attempting to render entity {:?}", entity);
        if let Some(page) = pages.pages.get_mut(&entity) {
            // Get the display list from the page
            let display_list = page.display_list_retained_snapshot();

            info!(
                "Rendering ValorUi with {} display items",
                display_list.items.len()
            );

            let width = valor_ui.width;
            let height = valor_ui.height;

            // Render using unified rendering function
            info!(
                "Calling render_display_list_to_pixels for entity {:?} with {}x{}",
                entity, width, height
            );
            let image_data =
                render_display_list_to_pixels(&mut pages, entity, &display_list, width, height);
            info!("Rendered {} bytes of image data", image_data.len());

            if let Some(existing) = existing_texture {
                // Update existing texture
                if let Some(image) = images.get_mut(&existing.image_handle) {
                    image.resize(Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    });
                    image.data = Some(image_data);
                }
                info!(
                    "Updated existing texture for ValorUi entity {:?} (size: {}x{})",
                    entity, width, height
                );
            } else {
                // Create new texture and display node
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
                info!(
                    "Creating display node with: width: {}px, height: {}px, position: Absolute, left: 0, top: 0",
                    width, height
                );
                let display_node = commands
                    .spawn((
                        Node {
                            width: Val::Px(width as f32),
                            height: Val::Px(height as f32),
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            ..Default::default()
                        },
                        ImageNode::new(image_handle.clone()),
                        bevy::ui::FocusPolicy::Pass, // Allow mouse events to pass through to window
                        Visibility::Visible,
                        ViewVisibility::default(),
                        InheritedVisibility::default(),
                    ))
                    .id();

                // Add the texture component to track it
                commands.entity(entity).insert(ValorTexture {
                    image_handle,
                    display_node,
                });

                info!(
                    "Created texture for ValorUi entity {:?} (size: {}x{}) with display node {:?}",
                    entity, width, height, display_node
                );
            }
        }

        // Remove NeedsRender marker after rendering
        info!("Removing NeedsRender from entity {:?}", entity);
        commands.entity(entity).remove::<NeedsRender>();
    }
}

/// Debug system to test if ANY input is being received
pub fn test_any_input(
    keys: Res<bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>>,
    mouse: Res<bevy::input::ButtonInput<bevy::input::mouse::MouseButton>>,
    windows: Query<&bevy::window::Window>,
) {
    if keys.get_just_pressed().len() > 0 {
        info!("⌨️ Key press detected!");
    }

    if mouse.get_just_pressed().len() > 0 {
        info!("🖱️ Raw mouse button press detected in test_any_input!");
        if let Ok(window) = windows.single() {
            if let Some(pos) = window.cursor_position() {
                info!("🖱️ Cursor position: ({}, {})", pos.x, pos.y);
            } else {
                warn!("🖱️ Cursor position is None!");
            }
        }
    }
}

/// Dispatches a click event to all Valor UI entities.
fn dispatch_click_to_uis(
    commands: &mut Commands,
    valor_uis: &Query<Entity, With<ValorUi>>,
    cursor_x: f32,
    cursor_y: f32,
) {
    info!("🖱️ Click at position: ({cursor_x}, {cursor_y})");
    for valor_ui_entity in valor_uis {
        info!(
            "🖱️ Dispatching click to ValorUi entity {:?}",
            valor_ui_entity
        );
        let button = 0; // Left button
        commands.queue(move |world: &mut World| {
            super::api::dispatch_click(world, valor_ui_entity, cursor_x, cursor_y, button);
        });
    }
}

/// System that handles mouse button input and dispatches clicks to Valor UIs
pub fn handle_mouse_clicks(
    mouse_button_input: Res<bevy::input::ButtonInput<bevy::input::mouse::MouseButton>>,
    windows: Query<&bevy::window::Window>,
    valor_uis: Query<Entity, With<ValorUi>>,
    mut commands: Commands,
) {
    use bevy::input::mouse::MouseButton;

    if !mouse_button_input.just_pressed(MouseButton::Left) {
        return;
    }

    info!("🖱️ Mouse click detected!");

    let Ok(window) = windows.single() else {
        warn!("🖱️ Click detected but no window found");
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        warn!("🖱️ Click detected but no cursor position");
        return;
    };

    dispatch_click_to_uis(&mut commands, &valor_uis, cursor_pos.x, cursor_pos.y);
}

/// System that handles window resize events and updates Valor page viewports
pub fn handle_window_resize(
    mut commands: Commands,
    mut valor_query: Query<(&mut ValorUi, Entity), With<ValorPageInitialized>>,
    windows: Query<&Window, Changed<Window>>,
) {
    for window in &windows {
        let width = window.width() as u32;
        let height = window.height() as u32;

        for (mut valor_ui, entity) in &mut valor_query {
            if valor_ui.width != width || valor_ui.height != height {
                info!(
                    "🪟 Window resized: {}x{} -> {}x{}",
                    valor_ui.width, valor_ui.height, width, height
                );

                // Update ValorUi dimensions
                valor_ui.width = width;
                valor_ui.height = height;

                // Queue a viewport update and re-render
                commands.queue(move |world: &mut World| {
                    handle_resize_for_entity(world, entity, width, height);
                });
            }
        }
    }
}

/// Handle resize for a specific entity
fn handle_resize_for_entity(world: &mut World, entity: Entity, width: u32, height: u32) {
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

        info!(
            "✅ Updated viewport and recomputed layout for entity {:?}",
            entity
        );

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

                if let Some(mut images) = world.get_resource_mut::<Assets<Image>>()
                    && let Some(image) = images.get_mut(&image_handle)
                {
                    // Update texture size and data
                    image.resize(bevy::render::render_resource::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    });
                    image.data = Some(image_data);
                    info!("✅ Updated texture after window resize to {width}x{height}");
                }
            }
            Err(err) => {
                error!("Failed to render after resize: {}", err);
            }
        }
    }
}

/// System that loads image assets requested via ImageAssetRequest components
pub fn load_image_assets(
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
