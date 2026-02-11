//! Theme switching example with dark mode toggle

use bevy::prelude::*;
use bevy::window::{Window, WindowPlugin};
use log::info;
use valor_dsl::jsx;
use valor_dsl::reactive::Component;
use valor_dsl::reactive::prelude::*;
use valor_dsl::reactive::runtime::ReactiveAppExt;

// Counter component with theme switching
#[derive(Component)]
struct ThemedCounter {
    value: i32,
    dark_mode: bool,
}

impl valor_dsl::reactive::Component for ThemedCounter {
    fn render(ui: &mut UiContext<Self>) -> Html {
        let count = ui.use_state().value;
        let dark_mode = ui.use_state().dark_mode;

        info!(
            "🎨 Rendering ThemedCounter (dark_mode: {}, count: {})",
            dark_mode, count
        );

        // Register event handlers
        let increment = ui.on_click("increment", |counter: &mut ThemedCounter| {
            counter.value += 1;
            info!("✨ Counter incremented to: {}", counter.value);
        });

        let decrement = ui.on_click("decrement", |counter: &mut ThemedCounter| {
            counter.value -= 1;
            info!("✨ Counter decremented to: {}", counter.value);
        });

        let reset = ui.on_click("reset", |counter: &mut ThemedCounter| {
            counter.value = 0;
            info!("✨ Counter reset to 0");
        });

        let toggle_theme = ui.on_click("toggle_theme", |counter: &mut ThemedCounter| {
            counter.dark_mode = !counter.dark_mode;
            info!("🌓 Theme toggled - dark mode: {}", counter.dark_mode);
        });

        // Theme-specific styles
        let (bg_gradient, text_color, card_bg, button_style) = if dark_mode {
            (
                "linear-gradient(135deg, #1e293b 0%, #0f172a 100%)",
                "white",
                "#334155",
                "background: #3b82f6; color: white;",
            )
        } else {
            (
                "linear-gradient(135deg, var(--color-primary) 0%, var(--color-secondary) 100%)",
                "white",
                "white",
                "background: white; color: var(--color-primary);",
            )
        };

        // JSX with dynamic theming
        jsx! {
            <div class="flex flex-col items-center justify-center min-h-screen p-10 text-center transition"
                 style={format!("background: {}; color: {};", bg_gradient, text_color)}>
                <style>"
                    .gradient-text {
                        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        -webkit-background-clip: text;
                        -webkit-text-fill-color: transparent;
                    }
                    .dark-gradient-text {
                        background: linear-gradient(135deg, #60a5fa 0%, #a78bfa 100%);
                        -webkit-background-clip: text;
                        -webkit-text-fill-color: transparent;
                    }
                "</style>

                <div class="p-12 rounded-2xl shadow-xl transition"
                     style={format!("background: {};", card_bg)}>
                    <h1 class="text-5xl font-bold text-shadow mb-5">
                        {if dark_mode { "🌙 Dark Mode Counter" } else { "☀️ Light Mode Counter" }}
                    </h1>

                    <div class={if dark_mode { "text-7xl font-bold my-10 dark-gradient-text" } else { "text-7xl font-bold my-10 gradient-text" }}>
                        {count}
                    </div>

                    <div class="flex gap-3 mb-5">
                        <button class="px-6 py-4 text-lg font-bold rounded-lg shadow
                                       hover:shadow-lg transition cursor-pointer"
                                style={button_style}
                                onclick={&increment}>
                            "➕ Increment"
                        </button>
                        <button class="px-6 py-4 text-lg font-bold rounded-lg shadow
                                       hover:shadow-lg transition cursor-pointer"
                                style={button_style}
                                onclick={&decrement}>
                            "➖ Decrement"
                        </button>
                        <button class="px-6 py-4 text-lg font-bold rounded-lg shadow
                                       hover:shadow-lg transition cursor-pointer"
                                style="background: var(--color-error); color: white;"
                                onclick={&reset}>
                            "🔄 Reset"
                        </button>
                    </div>

                    <button class="px-8 py-3 text-base font-semibold rounded-lg shadow-md
                                   hover:shadow-lg transition cursor-pointer"
                            style={if dark_mode { "background: #f59e0b; color: white;" } else { "background: #1e293b; color: white;" }}
                            onclick={&toggle_theme}>
                        {if dark_mode { "☀️ Switch to Light Mode" } else { "🌙 Switch to Dark Mode" }}
                    </button>

                    <p class="text-base opacity-90 mt-5">
                        "✨ Reactive theming with auto re-rendering!"
                    </p>
                </div>
            </div>
        }
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Spawn the themed counter component (starts in light mode)
    commands.spawn(ThemedCounter {
        value: 0,
        dark_mode: false,
    });

    info!("🎉 Theme switcher initialized!");
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Theme Switcher".to_string(),
                resolution: (900.0, 700.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(valor_dsl::reactive::ReactiveUiPlugin)
        .add_reactive_component(ThemedCounter::render)
        .add_systems(Startup, setup)
        .run();
}
