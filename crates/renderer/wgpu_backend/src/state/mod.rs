//! Main rendering state module.
//!
//! This module contains the `RenderState` struct which coordinates all rendering components.
//! Instead of being a god object, `RenderState` now uses composition to delegate
//! responsibilities to focused components.

// Component modules
pub(crate) mod error_scope;
pub(crate) mod gpu_context;
pub(crate) mod offscreen_target;
pub(crate) mod opacity_compositor;
pub(crate) mod pipeline_cache;
pub(crate) mod rectangle_renderer;
pub(crate) mod render_orchestrator;
pub(crate) mod resource_tracker;
pub(crate) mod text_renderer_state;

// Internal split modules
mod initialization;
mod rendering;
mod surface_management;

// Re-export commonly used items
use gpu_context::GpuContext;
use offscreen_target::OffscreenTarget;
use pipeline_cache::PipelineCache;
use rectangle_renderer::RectangleRenderer;
use renderer::display_list::{DisplayItem, DisplayList};
use renderer::renderer::{DrawRect, DrawText};
use resource_tracker::ResourceTracker;
pub use text_renderer_state::GlyphBounds;
use text_renderer_state::TextRendererState;
use wgpu::*;
use winit::dpi::PhysicalSize;

/// Composite info for a pre-rendered opacity group.
pub(crate) type OpacityComposite = (
    usize,
    usize,
    Texture,
    TextureView,
    u32,
    u32,
    f32,
    Bounds,
    BindGroup,
);

/// Compact representation for preprocessed layer data.
pub(crate) type LayerEntry = Option<(Vec<DisplayItem>, Vec<OpacityComposite>, Vec<(usize, usize)>)>;

/// Type alias for scissor rectangle: (x, y, width, height).
pub(crate) type ScissorRect = (u32, u32, u32, u32);

/// Parameters for offscreen rendering passes.
pub(crate) struct OffscreenRenderParams<'render> {
    pub(crate) encoder: &'render mut CommandEncoder,
    pub(crate) view: &'render TextureView,
    pub(crate) translated_items: &'render [DisplayItem],
    pub(crate) tex_width: u32,
    pub(crate) tex_height: u32,
    pub(crate) ctx: RenderContext,
}

/// Parameters for rendering rectangles pass.
pub(super) struct RenderRectanglesParams<'render_pass> {
    pub(super) encoder: &'render_pass mut CommandEncoder,
    pub(super) texture_view: &'render_pass TextureView,
    pub(super) use_retained: bool,
    pub(super) use_layers: bool,
    pub(super) is_offscreen: bool,
    pub(super) main_load: LoadOp<Color>,
}

/// Parameters for text rendering pass.
pub(super) struct RenderTextParams<'text_pass> {
    pub(super) encoder: &'text_pass mut CommandEncoder,
    pub(super) texture_view: &'text_pass TextureView,
    pub(super) text_load: LoadOp<Color>,
    pub(super) use_retained: bool,
    pub(super) use_layers: bool,
}

/// Vertex structure for texture quad rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TexVertex {
    pub(crate) pos: [f32; 2],
    pub(crate) tex_coords: [f32; 2],
}

/// Render performance metrics for observability and debugging.
#[derive(Debug, Clone, Default)]
pub struct RenderMetrics {
    pub frame_time_ms: f64,
    pub draw_calls: u32,
    pub vertices_rendered: u32,
    pub texture_memory_bytes: u64,
    pub error_count: u32,
    pub opacity_groups_rendered: u32,
}

/// Limits to prevent pathological content from exhausting resources.
#[derive(Debug, Clone)]
pub struct RenderLimits {
    pub max_display_items: usize,
    pub max_texture_size: u32,
    pub max_draw_calls_per_frame: u32,
    pub max_nested_opacity_groups: u32,
    pub max_texture_memory_bytes: u64,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_display_items: 100_000,
            max_texture_size: 8192,
            max_draw_calls_per_frame: 10_000,
            max_nested_opacity_groups: 32,
            max_texture_memory_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Feature flags for graceful degradation when GPU is struggling.
#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub opacity_compositing: bool,
    pub text_rendering: bool,
    pub complex_transforms: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            opacity_compositing: true,
            text_rendering: true,
            complex_transforms: true,
        }
    }
}

/// Rendering context that encapsulates viewport and size information.
#[derive(Debug, Copy, Clone)]
pub(crate) struct RenderContext {
    viewport_size: PhysicalSize<u32>,
}

impl RenderContext {
    pub(crate) const fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            viewport_size: size,
        }
    }

    pub(crate) fn width(self) -> u32 {
        self.viewport_size.width.max(1)
    }

    pub(crate) fn height(self) -> u32 {
        self.viewport_size.height.max(1)
    }
}

#[derive(Debug, Clone)]
pub enum Layer {
    Background,
    Content(DisplayList),
    Chrome(DisplayList),
}

/// `RenderState` coordinates rendering components using composition instead of inheritance.
/// Each component has a single, well-defined responsibility.
pub struct RenderState {
    /// GPU context managing device, queue, surface, and window.
    pub(super) gpu: GpuContext,
    /// Pipeline cache managing all rendering pipelines.
    pub(super) pipelines: PipelineCache,
    /// Text rendering state managing glyphon resources.
    pub(super) text: TextRendererState,
    /// Rectangle renderer managing vertex buffers.
    pub(super) rectangles: RectangleRenderer,
    /// Offscreen rendering target managing offscreen textures and readback.
    pub(super) offscreen: OffscreenTarget,
    /// Resource tracker keeping GPU resources alive through submission.
    pub(super) resources: ResourceTracker,

    /// Display list of rectangles to render (immediate mode).
    pub(super) display_list: Vec<DrawRect>,
    /// Display list of text items to render (immediate mode).
    pub(super) text_list: Vec<DrawText>,
    /// Retained display list (when set, becomes source of truth).
    pub(super) retained_display_list: Option<DisplayList>,
    /// Optional layers for multi-DL compositing.
    pub(super) layers: Vec<Layer>,
    /// Clear color for the framebuffer (canvas background). RGBA in [0,1].
    pub(super) clear_color: [f32; 4],
}

/// Pixel bounds (x, y, width, height)
pub(crate) type Bounds = (f32, f32, f32, f32);
