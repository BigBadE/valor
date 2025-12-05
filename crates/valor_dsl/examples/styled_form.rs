//! Styled form example using Valor DSL with Bevy

use bevy::prelude::*;
use std::sync::{Arc, Mutex};
use valor_dsl::bevy_integration::*;
use valor_dsl::events::{EventCallbacks, EventContext};

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Form Example".into(),
                resolution: (1024.0, 768.0).into(),
                ..default()
            }),
            ..default()
        }))
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

fn setup_form(
    mut commands: Commands,
    form_state: Res<FormState>,
) {
    commands.spawn(Camera2d);

    let html = r#"
        <html>
            <head>
                <style>
                    * {
                        box-sizing: border-box;
                        margin: 0;
                        padding: 0;
                    }
                    body {
                        font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
                        background: linear-gradient(120deg, #84fab0 0%, #8fd3f4 100%);
                        display: flex;
                        justify-content: center;
                        align-items: center;
                        min-height: 100vh;
                        padding: 20px;
                    }
                    .form-container {
                        background: white;
                        border-radius: 20px;
                        box-shadow: 0 10px 40px rgba(0, 0, 0, 0.1);
                        padding: 40px;
                        width: 500px;
                        max-width: 100%;
                    }
                    h1 {
                        color: #333;
                        margin-bottom: 10px;
                        font-size: 28px;
                    }
                    .subtitle {
                        color: #666;
                        margin-bottom: 30px;
                        font-size: 14px;
                    }
                    .form-group {
                        margin-bottom: 25px;
                    }
                    label {
                        display: block;
                        margin-bottom: 8px;
                        color: #444;
                        font-weight: 600;
                        font-size: 14px;
                    }
                    input[type="text"],
                    input[type="email"],
                    textarea {
                        width: 100%;
                        padding: 12px 15px;
                        border: 2px solid #e0e0e0;
                        border-radius: 10px;
                        font-size: 14px;
                        transition: all 0.3s ease;
                        font-family: inherit;
                    }
                    input[type="text"]:focus,
                    input[type="email"]:focus,
                    textarea:focus {
                        outline: none;
                        border-color: #84fab0;
                        box-shadow: 0 0 0 3px rgba(132, 250, 176, 0.1);
                    }
                    textarea {
                        resize: vertical;
                        min-height: 120px;
                    }
                    .button-group {
                        display: flex;
                        gap: 10px;
                        margin-top: 30px;
                    }
                    button {
                        flex: 1;
                        padding: 14px;
                        border: none;
                        border-radius: 10px;
                        font-size: 16px;
                        font-weight: 600;
                        cursor: pointer;
                        transition: all 0.3s ease;
                    }
                    .submit-btn {
                        background: linear-gradient(135deg, #84fab0 0%, #8fd3f4 100%);
                        color: white;
                    }
                    .submit-btn:hover {
                        transform: translateY(-2px);
                        box-shadow: 0 5px 20px rgba(132, 250, 176, 0.4);
                    }
                    .clear-btn {
                        background: #f0f0f0;
                        color: #666;
                    }
                    .clear-btn:hover {
                        background: #e0e0e0;
                    }
                    .input-hint {
                        font-size: 12px;
                        color: #999;
                        margin-top: 5px;
                    }
                    .required {
                        color: #ff6b6b;
                    }
                </style>
            </head>
            <body>
                <div class="form-container">
                    <h1>Contact Us</h1>
                    <p class="subtitle">We'd love to hear from you!</p>

                    <form>
                        <div class="form-group">
                            <label for="name">Name <span class="required">*</span></label>
                            <input
                                type="text"
                                id="name"
                                name="name"
                                placeholder="Enter your name"
                                on:input="update_name"
                            />
                            <div class="input-hint">Please enter your full name</div>
                        </div>

                        <div class="form-group">
                            <label for="email">Email <span class="required">*</span></label>
                            <input
                                type="email"
                                id="email"
                                name="email"
                                placeholder="your.email@example.com"
                                on:input="update_email"
                            />
                            <div class="input-hint">We'll never share your email</div>
                        </div>

                        <div class="form-group">
                            <label for="message">Message <span class="required">*</span></label>
                            <textarea
                                id="message"
                                name="message"
                                placeholder="Type your message here..."
                                on:input="update_message"
                            ></textarea>
                            <div class="input-hint">Minimum 10 characters</div>
                        </div>

                        <div class="button-group">
                            <button type="button" class="clear-btn" on:click="clear_form">Clear</button>
                            <button type="button" class="submit-btn" on:click="submit_form">Submit</button>
                        </div>
                    </form>
                </div>
            </body>
        </html>
    "#;

    let mut callbacks = EventCallbacks::new();

    let name_clone = form_state.name.clone();
    callbacks.register("update_name", move |_ctx: &EventContext| {
        let mut name = name_clone.lock().unwrap();
        *name = "User".to_string(); // Simplified
        info!("Name updated");
    });

    let email_clone = form_state.email.clone();
    callbacks.register("update_email", move |_ctx: &EventContext| {
        let mut email = email_clone.lock().unwrap();
        *email = "user@example.com".to_string(); // Simplified
        info!("Email updated");
    });

    let message_clone = form_state.message.clone();
    callbacks.register("update_message", move |_ctx: &EventContext| {
        let mut message = message_clone.lock().unwrap();
        *message = "Hello!".to_string(); // Simplified
        info!("Message updated");
    });

    let name_clone = form_state.name.clone();
    let email_clone = form_state.email.clone();
    let message_clone = form_state.message.clone();
    callbacks.register("submit_form", move |_ctx: &EventContext| {
        let name = name_clone.lock().unwrap();
        let email = email_clone.lock().unwrap();
        let message = message_clone.lock().unwrap();
        info!("Form submitted - Name: {}, Email: {}, Message: {}", *name, *email, *message);
    });

    let name_clone = form_state.name.clone();
    let email_clone = form_state.email.clone();
    let message_clone = form_state.message.clone();
    callbacks.register("clear_form", move |_ctx: &EventContext| {
        *name_clone.lock().unwrap() = String::new();
        *email_clone.lock().unwrap() = String::new();
        *message_clone.lock().unwrap() = String::new();
        info!("Form cleared");
    });

    info!("Form app started with HTML UI");
}
