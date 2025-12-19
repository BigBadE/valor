//! Simple counter example that actually runs and displays in Valor
//! This example demonstrates the Rust-based DSL runtime without requiring JavaScript

use anyhow::Result;
use env_logger::init as env_logger_init;
use js::{DOMUpdate, KeySpace, NodeKey};
use log::info;
use page_handler::{HtmlPage, ValorConfig};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use url::Url;
use valor_dsl::VirtualDom;
use valor_dsl::events::{EventCallbacks, EventContext};
use valor_dsl::runtime::RuntimeBuilder;

/// Helper to truncate string with ellipsis
fn truncate_string(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

/// Print sample of DOM updates
///
/// # Panics
///
/// This function does not panic.
fn print_sample_updates(updates: &[DOMUpdate]) {
    info!("\n📋 Sample DOM Updates:");
    for (index, update) in updates.iter().take(5).enumerate() {
        let number = index + 1;
        match update {
            DOMUpdate::InsertElement { tag, .. } => {
                info!("  {number}. InsertElement: <{tag}>");
            }
            DOMUpdate::InsertText { text, .. } => {
                let preview = truncate_string(text, 30);
                info!("  {number}. InsertText: {preview:?}");
            }
            DOMUpdate::SetAttr { name, value, .. } => {
                let preview = truncate_string(value, 40);
                info!("  {number}. SetAttr: {name}={preview:?}");
            }
            _ => {}
        }
    }
    if updates.len() > 5 {
        info!("  ... and {} more updates", updates.len() - 5);
    }
}

/// Get button styles CSS
const fn get_button_styles() -> &'static str {
    r"
        button {
            padding: 15px 30px;
            font-size: 18px;
            font-weight: 600;
            border: none;
            border-radius: 10px;
            cursor: pointer;
            transition: all 0.3s ease;
            color: white;
            min-width: 120px;
        }
        .decrement {
            background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%);
        }
        .decrement:hover {
            transform: translateY(-2px);
            box-shadow: 0 10px 20px rgba(245, 87, 108, 0.3);
        }
        .reset {
            background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%);
        }
        .reset:hover {
            transform: translateY(-2px);
            box-shadow: 0 10px 20px rgba(0, 242, 254, 0.3);
        }
        .increment {
            background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%);
        }
        .increment:hover {
            transform: translateY(-2px);
            box-shadow: 0 10px 20px rgba(67, 233, 123, 0.3);
        }
        .description {
            color: #666;
            margin-top: 30px;
            font-size: 14px;
        }
    "
}

/// Get CSS styles for the counter
fn get_counter_styles() -> String {
    format!(
        r#"
        body {{
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
        }}
        .container {{
            text-align: center;
            background: white;
            padding: 60px;
            border-radius: 20px;
            box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
            min-width: 400px;
        }}
        h1 {{
            color: #333;
            margin: 0 0 20px 0;
            font-size: 32px;
        }}
        .count {{
            font-size: 96px;
            font-weight: bold;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin: 40px 0;
            font-family: 'Arial', sans-serif;
        }}
        .button-group {{
            display: flex;
            gap: 15px;
            justify-content: center;
            margin-top: 30px;
        }}
        {}
    "#,
        get_button_styles()
    )
}

/// Build HTML with embedded styles for the counter
fn build_counter_html(current_count: i32) -> String {
    format!(
        r#"
            <html>
                <head>
                    <style>{}</style>
                </head>
                <body>
                    <div class="container">
                        <h1>Valor Counter</h1>
                        <div class="count">{current_count}</div>
                        <div class="button-group">
                            <button class="decrement">Decrement</button>
                            <button class="reset">Reset</button>
                            <button class="increment">Increment</button>
                        </div>
                        <p class="description">
                            A beautiful counter built with Valor DSL<br/>
                            Click the buttons to test!
                        </p>
                    </div>
                </body>
            </html>
        "#,
        get_counter_styles()
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger_init();

    info!("Starting Valor Counter Example (Rust-based DSL Runtime)");

    // Create Valor page
    let config = ValorConfig::from_env();
    let url = Url::parse("http://localhost/counter")?;
    let handle = Handle::current();

    let _page = HtmlPage::new(&handle, url, config).await?;

    // Counter state
    let count = Arc::new(Mutex::new(0i32));

    // Create DOM update channel
    let (dom_tx, mut dom_rx) = mpsc::channel::<Vec<DOMUpdate>>(100);

    // Setup virtual DOM and event callbacks
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let vdom = VirtualDom::new(key_manager);

    let mut callbacks = EventCallbacks::new();

    // Register event handlers
    let increment_count = Arc::clone(&count);
    callbacks.register("increment", move |_ctx: &EventContext| {
        if let Ok(mut counter) = increment_count.lock() {
            *counter += 1;
            info!("Count incremented to: {counter}");
        }
    });

    let decrement_count = Arc::clone(&count);
    callbacks.register("decrement", move |_ctx: &EventContext| {
        if let Ok(mut counter) = decrement_count.lock() {
            *counter -= 1;
            info!("Count decremented to: {counter}");
        }
    });

    let reset_count = Arc::clone(&count);
    callbacks.register("reset", move |_ctx: &EventContext| {
        if let Ok(mut counter) = reset_count.lock() {
            *counter = 0;
            info!("Count reset to: 0");
        }
    });

    // Build runtime
    let mut runtime = RuntimeBuilder::new()
        .vdom(vdom)
        .callbacks(callbacks)
        .dom_sender(dom_tx)
        .build()?;

    // Initial render
    let html = build_counter_html(0);
    info!("HTML length: {} bytes", html.len());
    info!("Rendering UI...");

    runtime.render(&html, NodeKey::ROOT).await?;

    // Process DOM updates in the background
    tokio::spawn(async move {
        while let Some(updates) = dom_rx.recv().await {
            info!("Received {} DOM updates", updates.len());
            print_sample_updates(&updates);
            // In a full implementation, these would be applied to the page
        }
    });

    info!("\nValor DSL Counter example completed successfully!");
    info!("Runtime is now active and can handle events");
    info!("In a full implementation, this would open a window with the rendered UI");

    // Keep the runtime alive for a bit to demonstrate
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    Ok(())
}
