//! Todo list example using Valor DSL with Bevy

use bevy::prelude::*;
use bevy::winit::WinitSettings;
use log::error;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use valor_dsl::events::{EventCallbacks, EventContext};

/// Main entry point for the todo list example
fn main() {
    let Ok(runtime) = Runtime::new() else {
        error!("Failed to create Tokio runtime");
        return;
    };
    let _guard = runtime.enter();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Valor Todo List".into(),
                resolution: (1024.0, 768.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(WinitSettings::desktop_app())
        .insert_resource(TodoState {
            todos: Arc::new(Mutex::new(vec![
                Todo {
                    id: 1,
                    text: "Learn Valor DSL".into(),
                    completed: false,
                },
                Todo {
                    id: 2,
                    text: "Build amazing UIs".into(),
                    completed: false,
                },
            ])),
            next_id: Arc::new(Mutex::new(3)),
        })
        .add_systems(Startup, setup_todo_list)
        .run();
}

#[derive(Clone, Debug)]
struct Todo {
    completed: bool,
}

#[derive(Resource, Clone)]
struct TodoState {
    todos: Arc<Mutex<Vec<Todo>>>,
    next_id: Arc<Mutex<u32>>,
}

/// Setup the todo list UI
fn setup_todo_list(mut commands: Commands, _todo_state: Res<TodoState>) {
    commands.spawn(Camera2d);
    log::info!("Todo list app started");
}

fn _get_todo_html_template() -> &'static str {
    r#"
        <html>
            <head>
                <style>
                    body {
                        display: flex;
                        justify-content: center;
                        padding: 40px;
                        margin: 0;
                        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
                        min-height: 100vh;
                    }
                    .todo-app {
                        background: white;
                        border-radius: 15px;
                        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
                        padding: 30px;
                        width: 500px;
                        max-width: 100%;
                    }
                    h1 {
                        color: #333;
                        margin: 0 0 30px 0;
                        font-size: 32px;
                        text-align: center;
                    }
                    .input-group {
                        display: flex;
                        gap: 10px;
                        margin-bottom: 30px;
                    }
                    input[type="text"] {
                        flex: 1;
                        padding: 12px;
                        border: 2px solid #e0e0e0;
                        border-radius: 8px;
                        font-size: 14px;
                        transition: border-color 0.3s;
                    }
                    input[type="text"]:focus {
                        outline: none;
                        border-color: #667eea;
                    }
                    button {
                        padding: 12px 24px;
                        border: none;
                        border-radius: 8px;
                        font-size: 14px;
                        font-weight: 600;
                        cursor: pointer;
                        transition: all 0.3s;
                        background: #667eea;
                        color: white;
                    }
                    button:hover {
                        background: #5568d3;
                        transform: translateY(-2px);
                        box-shadow: 0 4px 12px rgba(102, 126, 234, 0.4);
                    }
                    .todo-list {
                        list-style: none;
                        padding: 0;
                        margin: 0;
                    }
                    .todo-item {
                        display: flex;
                        align-items: center;
                        gap: 15px;
                        padding: 15px;
                        background: #f8f9fa;
                        border-radius: 8px;
                        margin-bottom: 10px;
                        transition: all 0.3s;
                    }
                    .todo-item:hover {
                        background: #e9ecef;
                        transform: translateX(5px);
                    }
                    .todo-item.completed {
                        opacity: 0.6;
                    }
                    .todo-item.completed .todo-text {
                        text-decoration: line-through;
                        color: #999;
                    }
                    .checkbox {
                        width: 20px;
                        height: 20px;
                        cursor: pointer;
                    }
                    .todo-text {
                        flex: 1;
                        color: #333;
                        font-size: 16px;
                    }
                    .delete-btn {
                        padding: 6px 12px;
                        font-size: 12px;
                        background: #ff6b6b;
                    }
                    .delete-btn:hover {
                        background: #ee5a52;
                    }
                    .stats {
                        text-align: center;
                        margin-top: 20px;
                        padding-top: 20px;
                        border-top: 2px solid #e0e0e0;
                        color: #666;
                        font-size: 14px;
                    }
                </style>
            </head>
            <body>
                <div class="todo-app">
                    <h1>📝 Todo List</h1>
                    <div class="input-group">
                        <input type="text" id="todo-input" placeholder="What needs to be done?" />
                        <button on:click="add_todo">Add</button>
                    </div>
                    <ul class="todo-list">
                        <li class="todo-item">
                            <input type="checkbox" class="checkbox" />
                            <span class="todo-text">Learn Valor DSL</span>
                            <button class="delete-btn" on:click="delete_todo">Delete</button>
                        </li>
                        <li class="todo-item">
                            <input type="checkbox" class="checkbox" />
                            <span class="todo-text">Build amazing UIs</span>
                            <button class="delete-btn" on:click="delete_todo">Delete</button>
                        </li>
                    </ul>
                    <div class="stats">
                        <strong>2</strong> items left
                    </div>
                </div>
            </body>
        </html>
    "#;

    let mut callbacks = EventCallbacks::new();

    let add_todos = Arc::clone(&todo_state.todos);
    let add_next_id = Arc::clone(&todo_state.next_id);
    callbacks.register("add_todo", move |_ctx: &EventContext| {
        let (Ok(mut todos), Ok(mut next_id)) = (add_todos.lock(), add_next_id.lock()) else {
            error!("Failed to lock todo state mutexes");
            return;
        };

        todos.push(Todo {
            id: *next_id,
            text: "New todo".into(),
            completed: false,
        });
        *next_id += 1;

        info!("Added new todo. Total: {}", todos.len());
    });

    let delete_todos = Arc::clone(&todo_state.todos);
    callbacks.register("delete_todo", move |_ctx: &EventContext| {
        let Ok(mut todos) = delete_todos.lock() else {
            error!("Failed to lock todos mutex");
            return;
        };
        if !todos.is_empty() {
            todos.pop();
            info!("Deleted todo. Remaining: {}", todos.len());
        }
    });

    let toggle_todos = Arc::clone(&todo_state.todos);
    callbacks.register("toggle_todo", move |_ctx: &EventContext| {
        let Ok(mut todos) = toggle_todos.lock() else {
            error!("Failed to lock todos mutex");
            return;
        };
        if let Some(todo) = todos.first_mut() {
            todo.completed = !todo.completed;
            info!("Toggled todo completion");
        }
    });

    info!("Todo list app started with HTML UI");
}
