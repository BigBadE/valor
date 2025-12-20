//! Image gallery example demonstrating Bevy asset integration
//!
//! This example shows how to use Bevy's asset system to load and display images
//! within Valor UIs using the reactive component system.

use bevy::prelude::*;
use bevy::window::{Window, WindowPlugin};
use valor_dsl::reactive::Component;
use valor_dsl::reactive::prelude::*;
use valor_dsl::reactive::runtime::ReactiveAppExt;

// Gallery component with reactive state
#[derive(Component)]
struct ImageGallery {
    current_index: usize,
    images: Vec<String>,
}

impl ImageGallery {
    fn new(images: Vec<String>) -> Self {
        Self {
            current_index: 0,
            images,
        }
    }

    fn current_image(&self) -> &str {
        self.images
            .get(self.current_index)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    fn next(&mut self) {
        if !self.images.is_empty() {
            self.current_index = (self.current_index + 1) % self.images.len();
        }
    }

    fn previous(&mut self) {
        if !self.images.is_empty() {
            self.current_index = if self.current_index == 0 {
                self.images.len() - 1
            } else {
                self.current_index - 1
            };
        }
    }
}

impl valor_dsl::reactive::Component for ImageGallery {
    fn render(ui: &mut UiContext<Self>) -> Html {
        // Get values we need from state first
        let current_image = ui.use_state().current_image().to_string();
        let current_index = ui.use_state().current_index;
        let total = ui.use_state().images.len();

        // Register event handlers
        let next = ui.on_click("next_image", |gallery: &mut ImageGallery| {
            gallery.next();
            info!(
                "📸 Next image: {}/{}",
                gallery.current_index + 1,
                gallery.images.len()
            );
        });

        let previous = ui.on_click("prev_image", |gallery: &mut ImageGallery| {
            gallery.previous();
            info!(
                "📸 Previous image: {}/{}",
                gallery.current_index + 1,
                gallery.images.len()
            );
        });

        // HTML with image display
        // Note: reactive_html! macro is not yet implemented for the new Html API
        // Using Html::empty() as placeholder - proper implementation would parse
        // this HTML string into DOMUpdate events
        let _html_template = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Image Gallery</title>
                <style>
                    body {{
                        font-family: Arial, sans-serif;
                        padding: 20px;
                        text-align: center;
                        background: linear-gradient(135deg, #1e3c72 0%, #2a5298 100%);
                        color: white;
                        min-height: 100vh;
                        margin: 0;
                        display: flex;
                        flex-direction: column;
                        justify-content: center;
                        align-items: center;
                    }}
                    .gallery-container {{
                        max-width: 800px;
                        margin: 0 auto;
                    }}
                    .image-display {{
                        background: white;
                        padding: 20px;
                        border-radius: 12px;
                        margin: 20px 0;
                        box-shadow: 0 8px 16px rgba(0,0,0,0.3);
                    }}
                    .image-display img {{
                        max-width: 100%;
                        max-height: 400px;
                        border-radius: 8px;
                        display: block;
                        margin: 0 auto;
                    }}
                    .image-placeholder {{
                        width: 400px;
                        height: 300px;
                        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        border-radius: 8px;
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        font-size: 24px;
                        color: white;
                        margin: 0 auto;
                    }}
                    .controls {{
                        margin: 20px 0;
                    }}
                    button {{
                        font-size: 18px;
                        padding: 12px 24px;
                        margin: 0 10px;
                        cursor: pointer;
                        border: none;
                        background: white;
                        color: #1e3c72;
                        border-radius: 8px;
                        font-weight: bold;
                        transition: transform 0.1s, box-shadow 0.1s;
                        box-shadow: 0 4px 6px rgba(0,0,0,0.2);
                    }}
                    button:hover {{
                        transform: translateY(-2px);
                        box-shadow: 0 6px 12px rgba(0,0,0,0.3);
                    }}
                    button:active {{
                        transform: translateY(0);
                    }}
                    .counter {{
                        font-size: 20px;
                        margin: 10px 0;
                        opacity: 0.9;
                    }}
                    h1 {{
                        font-size: 42px;
                        margin-bottom: 10px;
                        text-shadow: 2px 2px 4px rgba(0,0,0,0.3);
                    }}
                </style>
            </head>
            <body>
                <div class="gallery-container">
                    <h1>🖼️ Image Gallery</h1>
                    <div class="counter">Image {} of {}</div>

                    <div class="image-display">
                        <div class="image-placeholder">
                            🎨 Image: {}
                        </div>
                    </div>

                    <div class="controls">
                        <button onclick="{}">⬅️ Previous</button>
                        <button onclick="{}">Next ➡️</button>
                    </div>

                    <p style="font-size: 14px; opacity: 0.8;">
                        ✨ Reactive image gallery with Bevy asset integration
                    </p>
                </div>
            </body>
            </html>
            "#,
            current_index + 1,
            total,
            current_image,
            previous,
            next
        );

        // TODO: Parse _html_template into DOMUpdate events
        Html::empty()
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Create gallery with sample image paths
    let images = vec![
        "assets/images/photo1.png".to_string(),
        "assets/images/photo2.png".to_string(),
        "assets/images/photo3.png".to_string(),
    ];

    // Spawn the gallery component
    commands.spawn(ImageGallery::new(images));

    info!("🎉 Image gallery initialized!");
    info!("📁 Place images in assets/images/ directory");
}

fn main() {
    info!("Starting Image Gallery Example");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Image Gallery".to_string(),
                resolution: (900.0, 700.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(valor_dsl::reactive::ReactiveUiPlugin)
        .add_reactive_component(ImageGallery::render)
        .add_systems(Startup, setup)
        .run();
}
