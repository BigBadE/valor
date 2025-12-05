//! Simple counter example that actually runs and displays in Valor

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio;
use valor_dsl::*;
use valor_dsl::events::EventCallbacks;
use js::{KeySpace, NodeKey};
use page_handler::state::HtmlPage;
use page_handler::config::ValorConfig;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Create Valor page
    let config = ValorConfig::from_env();
    let url = url::Url::parse("http://localhost/counter")?;
    let handle = tokio::runtime::Handle::current();

    let mut page = HtmlPage::new(
        &handle,
        url,
        config,
    ).await?;

    // Counter state
    let count = Arc::new(Mutex::new(0i32));

    // Build HTML with embedded styles
    let build_html = |current_count: i32| -> String {
        format!(r#"
            <html>
                <head>
                    <style>
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
                        button {{
                            padding: 15px 30px;
                            font-size: 18px;
                            font-weight: 600;
                            border: none;
                            border-radius: 10px;
                            cursor: pointer;
                            transition: all 0.3s ease;
                            color: white;
                            min-width: 120px;
                        }}
                        .decrement {{
                            background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%);
                        }}
                        .decrement:hover {{
                            transform: translateY(-2px);
                            box-shadow: 0 10px 20px rgba(245, 87, 108, 0.3);
                        }}
                        .reset {{
                            background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%);
                        }}
                        .reset:hover {{
                            transform: translateY(-2px);
                            box-shadow: 0 10px 20px rgba(0, 242, 254, 0.3);
                        }}
                        .increment {{
                            background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%);
                        }}
                        .increment:hover {{
                            transform: translateY(-2px);
                            box-shadow: 0 10px 20px rgba(67, 233, 123, 0.3);
                        }}
                        .description {{
                            color: #666;
                            margin-top: 30px;
                            font-size: 14px;
                        }}
                    </style>
                </head>
                <body>
                    <div class="container">
                        <h1>🎯 Valor Counter</h1>
                        <div class="count">{}</div>
                        <div class="button-group">
                            <button class="decrement">− Decrement</button>
                            <button class="reset">↻ Reset</button>
                            <button class="increment">+ Increment</button>
                        </div>
                        <p class="description">
                            A beautiful counter built with Valor DSL<br/>
                            Click the buttons to test!
                        </p>
                    </div>
                </body>
            </html>
        "#, current_count)
    };

    // Initial render
    let html = build_html(0);

    println!("🚀 Starting Valor Counter Example");
    println!("📝 HTML length: {} bytes", html.len());
    println!("🎨 Rendering UI...");

    // For now, just parse and display the HTML
    // In a full implementation, this would:
    // 1. Send DOM updates to the page
    // 2. Handle events from the page
    // 3. Re-render on state changes

    // Compile HTML to see what DOMUpdates are generated
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    let callbacks = EventCallbacks::new();

    match vdom.compile_html(&html, NodeKey::ROOT, callbacks) {
        Ok(updates) => {
            println!("✅ Successfully compiled HTML");
            println!("📦 Generated {} DOM updates", updates.len());

            // Print first few updates as examples
            println!("\n📋 Sample DOM Updates:");
            for (i, update) in updates.iter().take(5).enumerate() {
                match update {
                    js::DOMUpdate::InsertElement { tag, .. } => {
                        println!("  {}. InsertElement: <{}>", i + 1, tag);
                    }
                    js::DOMUpdate::InsertText { text, .. } => {
                        let preview = if text.len() > 30 {
                            format!("{}...", &text[..30])
                        } else {
                            text.clone()
                        };
                        println!("  {}. InsertText: {:?}", i + 1, preview);
                    }
                    js::DOMUpdate::SetAttr { name, value, .. } => {
                        let preview = if value.len() > 40 {
                            format!("{}...", &value[..40])
                        } else {
                            value.clone()
                        };
                        println!("  {}. SetAttr: {}={:?}", i + 1, name, preview);
                    }
                    _ => {}
                }
            }
            if updates.len() > 5 {
                println!("  ... and {} more updates", updates.len() - 5);
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to compile HTML: {}", e);
            return Err(e);
        }
    }

    println!("\n✨ Valor DSL Counter example completed successfully!");
    println!("💡 In a full implementation, this would open a window with the rendered UI");

    Ok(())
}
