//! Public API functions for Valor UI integration

use super::components::*;
use super::rendering::render_valor_ui_display_list;
use bevy::asset::Handle;
use bevy::prelude::*;
use js::DOMUpdate;
use log::{info, error, warn};

/// Command to trigger a re-render of a ValorUi entity
pub fn rerender_valor_ui(world: &mut World, valor_ui_entity: Entity) {
    // Get the components we need
    let (width, height, display_node, html, is_first_render) = {
        let valor_ui = world.get::<ValorUi>(valor_ui_entity);
        let Some(ui) = valor_ui else { return };
        let texture = world.get::<ValorTexture>(valor_ui_entity);
        let Some(tex) = texture else { return };
        (
            ui.width,
            ui.height,
            tex.display_node,
            ui.html.clone(),
            ui.first_render,
        )
    };

    // Get tokio handle before borrowing pages mutably
    let tokio_handle = world.get_resource::<TokioHandle>().unwrap().0.clone();

    // Get pages for DOM updates
    let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

    // For reactive components, apply DOMUpdates directly instead of re-parsing HTML
    // Remove nodes from previous render, then apply new updates

    info!(
        "🔄 Re-rendering reactive component ({} DOM updates, epoch {})",
        html.updates.len(),
        html.epoch
    );

    // Get the page
    let page = match pages.pages.get_mut(&valor_ui_entity) {
        Some(p) => p,
        None => {
            error!("No HtmlPage found for entity {:?}", valor_ui_entity);
            return;
        }
    };

    // For re-renders, we only update text content - don't re-insert elements
    let updates_to_apply = if is_first_render {
        // First render: apply all updates as-is
        html.updates.clone()
    } else {
        // Re-render: ONLY send UpdateText for dynamic text nodes
        transform_updates_for_rerender(&html.updates)
    };

    // Apply DOM updates (or skip if empty on re-render)
    if !updates_to_apply.is_empty() {
        if let Err(err) = page.send_dom_updates(updates_to_apply) {
            error!("Failed to apply DOM updates: {}", err);
            return;
        }
    }

    // Update the page to compute layout
    if let Err(err) = tokio_handle.block_on(async { page.update().await }) {
        error!("Failed to update page after DOM updates: {}", err);
        return;
    }

    let display_list = page.display_list_retained_snapshot();

    // Drop pages to release the borrow before accessing world
    drop(pages);

    // Mark as no longer first render
    if let Some(mut valor_ui) = world.get_mut::<ValorUi>(valor_ui_entity) {
        valor_ui.first_render = false;
    }

    // Re-borrow pages for the rest of the function
    let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

    // Render using unified rendering function
    let image_data = super::rendering::render_display_list_to_pixels(
        &mut pages,
        valor_ui_entity,
        &display_list,
        width,
        height,
    );

    // Drop the ValorPages borrow before accessing Assets
    drop(pages);

    // Create and update the image
    update_image_texture(
        world,
        valor_ui_entity,
        display_node,
        image_data,
        width,
        height,
    );

    // Re-extract click handlers from the DOM after updates
    re_extract_click_handlers(world, valor_ui_entity);
}

/// Transform DOM updates for re-render (convert InsertText to UpdateText)
fn transform_updates_for_rerender(updates: &[DOMUpdate]) -> Vec<DOMUpdate> {
    let mut transformed_updates = Vec::new();

    for update in updates {
        match update {
            DOMUpdate::InsertText { node, text, .. } => {
                // Convert to UpdateText for re-renders
                transformed_updates.push(DOMUpdate::UpdateText {
                    node: *node,
                    text: text.clone(),
                });
            }
            DOMUpdate::InsertElement { .. } => {
                // Skip - elements already exist from first render
                continue;
            }
            DOMUpdate::SetAttr { .. } => {
                // Skip - attributes already set from first render
                // TODO: In the future, we may want to diff and update changed attributes
                continue;
            }
            _ => {
                // Skip other operations on re-render
                continue;
            }
        }
    }

    info!(
        "🔄 Re-render: sending {} text updates (skipped {} element inserts)",
        transformed_updates.len(),
        updates
            .iter()
            .filter(|u| matches!(u, DOMUpdate::InsertElement { .. }))
            .count()
    );
    transformed_updates
}

/// Update the image texture with new pixel data
fn update_image_texture(
    world: &mut World,
    valor_ui_entity: Entity,
    display_node: Entity,
    image_data: Vec<u8>,
    width: u32,
    height: u32,
) {
    // Create a new Image with the updated data
    let new_image = super::rendering::create_bevy_image(image_data, width, height);

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
        info!(
            "✅ Re-rendered ValorUi entity {:?} and updated display node {:?}",
            valor_ui_entity, display_node
        );
    } else {
        warn!(
            "Failed to get ImageNode for display node {:?}",
            display_node
        );
    }
}

/// Re-extract click handlers after DOM updates
fn re_extract_click_handlers(world: &mut World, valor_ui_entity: Entity) {
    let mut pages = world.get_non_send_resource_mut::<ValorPages>().unwrap();

    // Clear existing handlers for this entity
    pages
        .click_handlers
        .retain(|(e, _), _| *e != valor_ui_entity);

    // Extract handlers from the parsed DOM
    let page = match pages.pages.get_mut(&valor_ui_entity) {
        Some(p) => p,
        None => return,
    };

    // Extract handlers from the layouter attributes map
    let attrs_map = page.layouter_attrs_map();

    for (node_key, attrs) in &attrs_map {
        if let Some(handler_name) = attrs.get("onclick") {
            info!(
                "🔄 Re-extracted click handler '{}' on node {:?}",
                handler_name, node_key
            );
            pages
                .click_handlers
                .insert((valor_ui_entity, *node_key), handler_name.clone());
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

    // After manually updating text, we need to re-render the display list
    // DON'T call rerender_valor_ui() because it tries to apply stale HTML structure
    // Instead, just re-render the pixels from the current DOM state
    let _ = render_valor_ui_display_list(world, valor_ui_entity);
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
        let contains_point =
            x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;

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


        for handler_entity in handler_entities {
            let event = crate::bevy_events::OnClick {
                node: js::NodeKey::ROOT,
                position: (x, y),
                button,
                entity: handler_entity,
            };
            info!(
                "Dispatching click event to handler '{}' (entity {:?})",
                handler_name, handler_entity
            );
            world.trigger(event.clone());
        }
    }
}

/// Public API: Load an image into the Valor UI system
/// Returns the image handle that can be used in Bevy UI or passed to Valor
pub fn load_image(commands: &mut Commands, source: impl Into<String>) -> Entity {
    let source = source.into();
    commands.spawn(ImageAssetRequest { source }).id()
}

/// Public API: Get an image handle by source path
pub fn get_image_handle(registry: &ImageRegistry, source: &str) -> Option<Handle<Image>> {
    registry.get_image(source)
}
