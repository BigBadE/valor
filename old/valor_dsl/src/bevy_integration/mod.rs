//! Bevy ECS integration for Valor DSL
//!
//! This module provides components, resources, and systems for rendering
//! HTML/CSS UIs within Bevy applications using the Valor browser engine.

use log::info;
mod api;
mod components;
mod rendering;
pub mod systems;

// Re-export public API
pub use api::{
    dispatch_click, get_element_text, get_image_handle, load_image, rerender_valor_ui,
    update_element_text,
};
pub use components::{
    ClickHandler, GlobalStyles, ImageAssetRequest, ImageRegistry, NeedsRender,
    PersistentRenderContext, TokioHandle, ValorPageInitialized, ValorPages, ValorTexture, ValorUi,
};

use crate::styling::{TailwindUtilities, Theme};
use bevy::prelude::*;
use systems::*;

/// Plugin to add Valor UI support to Bevy applications
pub struct ValorUiPlugin;

impl Plugin for ValorUiPlugin {
    fn build(&self, app: &mut App) {
        // Generate global CSS from theme and Tailwind utilities
        let theme = Theme::default();
        let global_css = format!(
            "{}\n{}",
            theme.to_css(),
            TailwindUtilities::generate(&theme.colors)
        );

        // Create a Tokio runtime for async HtmlPage operations
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        let handle = runtime.handle().clone();

        app.insert_non_send_resource(runtime)
            .insert_non_send_resource(ValorPages::default())
            .insert_resource(TokioHandle(handle))
            .insert_resource(ImageRegistry::default())
            .insert_resource(GlobalStyles(global_css))
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
