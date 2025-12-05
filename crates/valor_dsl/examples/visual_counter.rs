//! Visual counter example that renders using the Valor browser engine
//!
//! This creates an actual window and renders the counter UI.

use std::sync::Arc;
use tokio;
use winit::event_loop::{EventLoop, ControlFlow};
use winit::event::{Event, WindowEvent};
use winit::window::WindowBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    println!("🚀 Starting Valor Visual Counter");

    // Create event loop and window
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title("Valor Counter - DSL Example")
        .with_inner_size(winit::dpi::LogicalSize::new(800, 600))
        .build(&event_loop)?;

    let window = Arc::new(window);

    // Create Valor page with HTML
    use page_handler::state::HtmlPage;
    use page_handler::config::ValorConfig;

    let config = ValorConfig::from_env();
    let url = url::Url::parse("http://localhost/counter")?;
    let handle = tokio::runtime::Handle::current();

    let mut page = HtmlPage::new(
        &handle,
        url,
        config,
    ).await?;

    let html = r#"
        <!DOCTYPE html>
        <html>
            <head>
                <style>
                    * {
                        margin: 0;
                        padding: 0;
                        box-sizing: border-box;
                    }
                    body {
                        display: flex;
                        justify-content: center;
                        align-items: center;
                        height: 100vh;
                        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
                    }
                    .container {
                        text-align: center;
                        background: white;
                        padding: 60px 80px;
                        border-radius: 20px;
                        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
                    }
                    h1 {
                        color: #333;
                        font-size: 36px;
                        margin-bottom: 20px;
                    }
                    .count {
                        font-size: 120px;
                        font-weight: bold;
                        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        -webkit-background-clip: text;
                        -webkit-text-fill-color: transparent;
                        margin: 50px 0;
                    }
                    .button-group {
                        display: flex;
                        gap: 20px;
                        justify-content: center;
                        margin-top: 40px;
                    }
                    button {
                        padding: 18px 36px;
                        font-size: 20px;
                        font-weight: 600;
                        border: none;
                        border-radius: 12px;
                        cursor: pointer;
                        transition: all 0.3s ease;
                        color: white;
                        min-width: 140px;
                    }
                    .decrement {
                        background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%);
                    }
                    .reset {
                        background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%);
                    }
                    .increment {
                        background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%);
                    }
                    button:hover {
                        transform: translateY(-3px);
                        box-shadow: 0 12px 24px rgba(0, 0, 0, 0.2);
                    }
                    .info {
                        color: #666;
                        margin-top: 40px;
                        font-size: 16px;
                        line-height: 1.6;
                    }
                </style>
            </head>
            <body>
                <div class="container">
                    <h1>🎯 Valor Counter</h1>
                    <div class="count">0</div>
                    <div class="button-group">
                        <button class="decrement">−</button>
                        <button class="reset">↻</button>
                        <button class="increment">+</button>
                    </div>
                    <div class="info">
                        Built with Valor DSL<br/>
                        <strong>Pure HTML/CSS rendered by Valor browser engine</strong>
                    </div>
                </div>
            </body>
        </html>
    "#;

    println!("📄 Loading HTML into Valor page...");

    // Load the HTML into the page
    page.load_html(html.to_string()).await?;

    println!("✅ HTML loaded successfully");

    // Create renderer
    use wgpu_backend::WgpuRenderer;
    use pollster;

    let size = window.inner_size();
    let mut renderer = pollster::block_on(async {
        WgpuRenderer::new(window.clone(), size.width, size.height).await
    })?;

    println!("🎨 Renderer initialized");
    println!("👀 Window should now be visible!");
    println!("Press ESC or close window to exit");

    // Event loop
    event_loop.run(move |event, target| {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    println!("🛑 Closing window...");
                    target.exit();
                }
                WindowEvent::KeyboardInput { event: key_event, .. } => {
                    if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) = key_event.physical_key {
                        println!("🛑 ESC pressed, closing...");
                        target.exit();
                    }
                }
                WindowEvent::Resized(new_size) => {
                    renderer.resize(new_size.width, new_size.height);
                }
                WindowEvent::RedrawRequested => {
                    // Update page
                    let rt = tokio::runtime::Handle::current();
                    let update_result = rt.block_on(async {
                        page.update().await
                    });

                    if let Ok(outcome) = update_result {
                        if outcome.redraw_needed {
                            // Render the page
                            if let Some(display_list) = page.display_list() {
                                match renderer.render(display_list) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        eprintln!("Render error: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    })?;

    Ok(())
}
