//! Reactive counter example with JSX and auto re-rendering

use bevy::prelude::*;
use bevy::window::{Window, WindowPlugin};
use log::info;
use valor_dsl::jsx;
use valor_dsl::reactive::Component;
use valor_dsl::reactive::prelude::*;
use valor_dsl::reactive::runtime::ReactiveAppExt;

// Define your state as a simple Bevy component
#[derive(Component)]
struct Counter {
    value: i32,
}

// Implement the Component trait
impl valor_dsl::reactive::Component for Counter {
    fn render(ui: &mut UiContext<Self>) -> Html {
        let count = ui.use_state().value;
        info!("🎨 Rendering Counter with value: {}", count);

        // Register event handlers
        let increment = ui.on_click("increment", |counter: &mut Counter| {
            counter.value += 1;
            info!("✨ Counter incremented to: {}", counter.value);
        });

        let decrement = ui.on_click("decrement", |counter: &mut Counter| {
            counter.value -= 1;
            info!("✨ Counter decremented to: {}", counter.value);
        });

        let reset = ui.on_click("reset", |counter: &mut Counter| {
            counter.value = 0;
            info!("✨ Counter reset to 0");
        });

        // JSX with Tailwind utilities and minimal custom styles
        jsx! {
            <div class="flex flex-col items-center justify-center min-h-screen p-10 text-center text-white"
                 style="background: linear-gradient(135deg, var(--color-primary) 0%, var(--color-secondary) 100%);">
                <style>"
                    .gradient-text {
                        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        -webkit-background-clip: text;
                        -webkit-text-fill-color: transparent;
                    }
                "</style>

                <h1 class="text-5xl font-bold text-shadow mb-5">
                    "🚀 Reactive Counter"
                </h1>

                <div class="text-7xl font-bold my-10 gradient-text">
                    {count}
                </div>

                <div class="flex gap-3">
                    <button class="px-6 py-4 text-lg font-bold bg-white rounded-lg shadow
                                   hover:shadow-lg transition cursor-pointer"
                            style="color: var(--color-primary);"
                            onclick={&increment}>
                        "➕ Increment"
                    </button>
                    <button class="px-6 py-4 text-lg font-bold bg-white rounded-lg shadow
                                   hover:shadow-lg transition cursor-pointer"
                            style="color: var(--color-primary);"
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

                {
                    if count > 10 {
                        jsx!{ <p class="text-base opacity-90 mt-5">"🔥 You're on fire!"</p> }
                    } else if count < 0 {
                        jsx!{ <p class="text-base opacity-90 mt-5">"📉 Going negative!"</p> }
                    } else {
                        jsx!{ <p class="text-base opacity-90 mt-5">"✨ Reactive JSX with auto re-rendering!"</p> }
                    }
                }
            </div>
        }
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Just spawn the component - the reactive system handles the rest!
    commands.spawn(Counter { value: 0 });

    info!("🎉 Reactive counter initialized!");
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Reactive Counter".to_string(),
                resolution: (800.0, 600.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(valor_dsl::reactive::ReactiveUiPlugin)
        // Register the Counter component type with its render function
        .add_reactive_component(Counter::render)
        .add_systems(Startup, setup)
        .run();
}
