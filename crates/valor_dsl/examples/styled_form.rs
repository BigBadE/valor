//! Styled form example using Valor DSL with Bevy

use bevy::prelude::*;
use bevy::winit::WinitSettings;
use log::error;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use valor_dsl::events::{EventCallbacks, EventContext};

/// Main entry point for the styled form example
fn main() {
    let Ok(runtime) = Runtime::new() else {
        error!("Failed to create Tokio runtime");
        return;
    };
    let _guard = runtime.enter();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Form Example".into(),
                resolution: (1024.0, 768.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(WinitSettings::desktop_app())
        .insert_resource(FormState {
            name: Arc::new(Mutex::new(String::new())),
            email: Arc::new(Mutex::new(String::new())),
            message: Arc::new(Mutex::new(String::new())),
        })
        .add_systems(Startup, setup_form)
        .run();
}

#[derive(Resource, Clone)]
struct FormState {
    name: Arc<Mutex<String>>,
    email: Arc<Mutex<String>>,
    message: Arc<Mutex<String>>,
}

fn create_form_callbacks(form_state: &FormState) -> EventCallbacks {
    let mut callbacks = EventCallbacks::new();

    let name_update = Arc::clone(&form_state.name);
    callbacks.register("update_name", move |_ctx: &EventContext| {
        let Ok(mut name) = name_update.lock() else {
            error!("Failed to lock name mutex");
            return;
        };
        *name = "User".to_string();
        log::info!("Name updated");
    });

    let email_update = Arc::clone(&form_state.email);
    callbacks.register("update_email", move |_ctx: &EventContext| {
        let Ok(mut email) = email_update.lock() else {
            error!("Failed to lock email mutex");
            return;
        };
        *email = "user@example.com".to_string();
        log::info!("Email updated");
    });

    let message_update = Arc::clone(&form_state.message);
    callbacks.register("update_message", move |_ctx: &EventContext| {
        let Ok(mut message) = message_update.lock() else {
            error!("Failed to lock message mutex");
            return;
        };
        *message = "Hello!".to_string();
        log::info!("Message updated");
    });

    let submit_name = Arc::clone(&form_state.name);
    let submit_email = Arc::clone(&form_state.email);
    let submit_message = Arc::clone(&form_state.message);
    callbacks.register("submit_form", move |_ctx: &EventContext| {
        let (Ok(name), Ok(email), Ok(message)) = (
            submit_name.lock(),
            submit_email.lock(),
            submit_message.lock(),
        ) else {
            error!("Failed to lock form state mutexes");
            return;
        };
        log::info!("Form submitted - Name: {name}, Email: {email}, Message: {message}");
    });

    let clear_name = Arc::clone(&form_state.name);
    let clear_email = Arc::clone(&form_state.email);
    let clear_message = Arc::clone(&form_state.message);
    callbacks.register("clear_form", move |_ctx: &EventContext| {
        let (Ok(mut name), Ok(mut email), Ok(mut message)) =
            (clear_name.lock(), clear_email.lock(), clear_message.lock())
        else {
            error!("Failed to lock form state mutexes");
            return;
        };
        *name = String::new();
        *email = String::new();
        *message = String::new();
        log::info!("Form cleared");
    });

    callbacks
}

/// Setup the form UI
fn setup_form(mut commands: Commands, form_state: Res<FormState>) {
    commands.spawn(Camera2d);
    let _callbacks = create_form_callbacks(&form_state);
    log::info!("Form app started with HTML UI");
}
