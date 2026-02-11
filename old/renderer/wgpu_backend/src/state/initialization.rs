//! Initialization logic for `RenderState`.

use super::RenderState;
use super::gpu_context::GpuContext;
use super::offscreen_target::OffscreenTarget;
use super::pipeline_cache::PipelineCache;
use super::rectangle_renderer::RectangleRenderer;
use super::resource_tracker::ResourceTracker;
use super::text_renderer_state::TextRendererState;
use anyhow::Error as AnyhowError;
use std::sync::Arc;
use winit::window::Window;

impl RenderState {
    /// Create the GPU device/surface and initialize all rendering components.
    ///
    /// # Errors
    /// Returns an error if GPU initialization fails.
    pub async fn new(window: Arc<Window>) -> Result<Self, AnyhowError> {
        let gpu = GpuContext::new(Arc::clone(&window)).await?;
        let pipelines = PipelineCache::new(gpu.device(), gpu.render_format());
        let text =
            TextRendererState::new(gpu.device(), gpu.queue(), gpu.render_format(), gpu.size());
        let rectangles = RectangleRenderer::new(
            pipelines.initial_vertex_buffer().clone(),
            pipelines.initial_vertex_count(),
        );
        let offscreen = OffscreenTarget::new();
        let resources = ResourceTracker::new();

        Ok(Self {
            gpu,
            pipelines,
            text,
            rectangles,
            offscreen,
            resources,
            display_list: Vec::new(),
            text_list: Vec::new(),
            retained_display_list: None,
            layers: Vec::new(),
            clear_color: [1.0, 1.0, 1.0, 1.0],
        })
    }
}
