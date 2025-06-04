use tokio::runtime::Runtime;
use page_handler::state::PageState;
use wgpu_renderer::state::RenderState;

pub struct AppState {
    pub runtime: Runtime,
    pub render_state: RenderState,
    pub pages: Vec<PageState>
}