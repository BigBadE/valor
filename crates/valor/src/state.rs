use js::ChromeHostCommand;
use page_handler::state::HtmlPage;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedReceiver;
use wgpu_backend::RenderState;

/// Global application state owned by the winit `ApplicationHandler`.
/// Holds the async runtime, renderer, and all active pages (index 0 = chrome, 1 = content).
/// Also maintains minimal input routing state for Phase 3.
pub struct AppState {
    /// Tokio async runtime for running async operations.
    pub runtime: Runtime,
    /// WGPU rendering state.
    pub render_state: RenderState,
    /// Active HTML pages (index 0 = chrome, 1 = content).
    pub pages: Vec<HtmlPage>,
    /// Receiver for privileged chromeHost commands.
    pub chrome_host_rx: UnboundedReceiver<ChromeHostCommand>,
    /// Index of the page currently receiving keyboard input (focus owner).
    pub focused_page_index: usize,
    /// Target page for the current pointer (updated on cursor move/press) if known.
    pub pointer_target_index: Option<usize>,
    /// Last known cursor position in logical pixels.
    pub last_cursor_pos: Option<(f64, f64)>,
    /// Height of the chrome top bar band in logical pixels used for simple routing.
    pub chrome_bar_height_px: f64,
}
