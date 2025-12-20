//! Rendering functions for Valor UI

use super::components::{PersistentRenderContext, ValorPages, ValorTexture, ValorUi};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use renderer::display_list::DisplayList;

/// Internal: Unified function to render a display list to RGBA pixels using persistent GPU context
pub(super) fn render_display_list_to_pixels(
    pages: &mut ValorPages,
    entity: Entity,
    display_list: &DisplayList,
    width: u32,
    height: u32,
) -> Vec<u8> {
    // Initialize persistent render context if not present
    if !pages.render_contexts.contains_key(&entity) {
        match wgpu_backend::initialize_persistent_context(width, height) {
            Ok(ctx) => {
                pages.render_contexts.insert(
                    entity,
                    PersistentRenderContext {
                        width,
                        height,
                        gpu_context: ctx,
                    },
                );
                info!("Initialized persistent GPU context for entity {:?}", entity);
            }
            Err(err) => {
                error!("Failed to initialize persistent GPU context: {}", err);
                return create_fallback_image(width, height);
            }
        }
    }

    // Render using persistent context
    let render_ctx = pages.render_contexts.get_mut(&entity).unwrap();
    match wgpu_backend::render_display_list_with_context(
        &mut render_ctx.gpu_context,
        display_list,
        width,
        height,
    ) {
        Ok(data) => data,
        Err(err) => {
            error!("Failed to render display list: {}", err);
            create_fallback_image(width, height)
        }
    }
}

/// Create a fallback gray image
fn create_fallback_image(width: u32, height: u32) -> Vec<u8> {
    let mut fallback = vec![255u8; (width * height * 4) as usize];
    for pixel in fallback.chunks_exact_mut(4) {
        pixel[0] = 240;
        pixel[1] = 240;
        pixel[2] = 245;
        pixel[3] = 255;
    }
    fallback
}

/// Internal: Re-render just the display list without modifying the DOM.
///
/// This is the correct function to call after incremental DOM updates (like update_element_text).
/// It recomputes layout and re-renders pixels without touching the DOM structure.
///
/// DO NOT call `rerender_valor_ui()` after incremental updates - that function is ONLY
/// for reactive components that regenerate their entire HTML structure on each render.
pub(super) fn render_valor_ui_display_list(
    world: &mut World,
    valor_ui_entity: Entity,
) -> Result<(), String> {
    // Get display node and dimensions first
    let (display_node, width, height) = {
        let texture = world
            .get::<ValorTexture>(valor_ui_entity)
            .ok_or("No ValorTexture component")?;
        let valor_ui = world
            .get::<ValorUi>(valor_ui_entity)
            .ok_or("No ValorUi component")?;
        (texture.display_node, valor_ui.width, valor_ui.height)
    };

    let tokio_handle = world
        .get_resource::<super::components::TokioHandle>()
        .ok_or("No TokioHandle resource")?
        .0
        .clone();

    // Get the display list from the page
    let display_list = {
        let mut pages = world
            .get_non_send_resource_mut::<ValorPages>()
            .ok_or("No ValorPages resource")?;
        let page = pages
            .pages
            .get_mut(&valor_ui_entity)
            .ok_or("No HtmlPage for entity")?;

        // Update the page to recompute layout after text change
        if let Err(err) = tokio_handle.block_on(async { page.update().await }) {
            error!("Failed to update page: {}", err);
            return Err("Failed to update page".to_string());
        }

        page.display_list_retained_snapshot()
    };

    // Render using unified rendering function
    let image_data = {
        let mut pages = world
            .get_non_send_resource_mut::<ValorPages>()
            .ok_or("No ValorPages resource")?;
        render_display_list_to_pixels(&mut pages, valor_ui_entity, &display_list, width, height)
    };

    // Create new image and update texture
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

    let mut images = world
        .get_resource_mut::<Assets<Image>>()
        .ok_or("No Assets<Image> resource")?;
    let new_handle = images.add(new_image);

    // Update texture handle
    if let Some(mut texture) = world.get_mut::<ValorTexture>(valor_ui_entity) {
        texture.image_handle = new_handle.clone();
    }

    // Update display node's image
    if let Some(mut image_node) = world.get_mut::<ImageNode>(display_node) {
        *image_node = ImageNode::new(new_handle);
    }

    Ok(())
}

/// Create a Bevy Image from rendered pixels
pub(super) fn create_bevy_image(image_data: Vec<u8>, width: u32, height: u32) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        image_data,
        TextureFormat::Rgba8UnormSrgb,
        Default::default(),
    )
}
