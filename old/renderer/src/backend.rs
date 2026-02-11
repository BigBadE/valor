//! Rendering backend abstraction.
//!
//! Defines the `RenderBackend` trait that allows swapping between different
//! GPU rendering implementations (WGPU, OpenGL, Software, etc.).

use crate::display_list::DisplayList;
use anyhow::Result as AnyResult;
use core::fmt::Debug;

/// Render target for presenting frames.
pub trait RenderTarget: Debug + Send {
    /// Get the width of the render target in physical pixels.
    fn width(&self) -> u32;

    /// Get the height of the render target in physical pixels.
    fn height(&self) -> u32;

    /// Resize the render target to the given dimensions.
    fn resize(&mut self, width: u32, height: u32);
}

/// Backend-agnostic rendering interface.
///
/// Implementations handle GPU resource management, command encoding,
/// and presentation for a specific graphics API.
pub trait RenderBackend: Debug + Send {
    /// Associated render target type for this backend.
    type Target: RenderTarget;

    /// Render a display list to the current render target.
    ///
    /// # Errors
    /// Returns an error if rendering or presentation fails.
    fn render(&mut self, display_list: &DisplayList) -> AnyResult<()>;

    /// Set a retained display list that will be rendered each frame.
    ///
    /// This allows the backend to optimize rendering of static content.
    fn set_retained_display_list(&mut self, display_list: DisplayList);

    /// Clear any cached retained display list.
    fn clear_retained_display_list(&mut self);

    /// Get a reference to the current render target.
    fn target(&self) -> &Self::Target;

    /// Get a mutable reference to the current render target.
    fn target_mut(&mut self) -> &mut Self::Target;

    /// Resize the render target and invalidate cached resources.
    fn resize(&mut self, width: u32, height: u32);

    /// Begin a new frame.
    ///
    /// # Errors
    /// Returns an error if frame acquisition fails.
    fn begin_frame(&mut self) -> AnyResult<()>;

    /// End the current frame and present to the screen.
    ///
    /// # Errors
    /// Returns an error if presentation fails.
    fn end_frame(&mut self) -> AnyResult<()>;

    /// Get rendering metrics (FPS, frame time, etc.).
    fn metrics(&self) -> BackendMetrics;

    /// Enable or disable debug overlays (wireframe, overdraw, etc.).
    fn set_debug_mode(&mut self, mode: DebugMode);
}

/// Rendering performance metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct BackendMetrics {
    /// Frames rendered in the last second.
    pub fps: f32,
    /// Average frame time in milliseconds.
    pub frame_time_ms: f32,
    /// Number of draw calls in the last frame.
    pub draw_calls: u32,
    /// Number of vertices rendered in the last frame.
    pub vertices: u32,
    /// GPU memory used in bytes.
    pub gpu_memory_bytes: u64,
}

/// Debug visualization modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugMode {
    /// No debug visualization.
    #[default]
    None,
    /// Show wireframe overlays.
    Wireframe,
    /// Show overdraw heatmap.
    Overdraw,
    /// Show layer boundaries.
    LayerBounds,
    /// Show stacking context boundaries.
    StackingContexts,
}
