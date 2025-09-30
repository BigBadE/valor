use page_handler::state::HtmlPage;
use tokio::runtime::Runtime;
use wgpu_backend::state::RenderState;

/// Global application state owned by the winit ApplicationHandler.
/// Holds the async runtime, renderer, and all active pages (index 0 = chrome, 1 = content).
/// Also maintains minimal input routing state for Phase 3.
pub struct AppState {
    pub runtime: Runtime,
    pub render_state: RenderState,
    pub pages: Vec<HtmlPage>,
    /// Receiver for privileged chromeHost commands.
    pub chrome_host_rx: tokio::sync::mpsc::UnboundedReceiver<js::ChromeHostCommand>,
    /// Index of the page currently receiving keyboard input (focus owner).
    pub focused_page_index: usize,
    /// Target page for the current pointer (updated on cursor move/press) if known.
    pub pointer_target_index: Option<usize>,
    /// Last known cursor position in logical pixels.
    pub last_cursor_pos: Option<(f64, f64)>,
    /// Height of the chrome top bar band in logical pixels used for simple routing.
    pub chrome_bar_height_px: f64,
}
