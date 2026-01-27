//! Backend trait for rendering display lists.
//!
//! This module defines the interface that rendering backends must implement.

use crate::display_list::DisplayList;

/// Trait for rendering backends.
pub trait RenderBackend {
    /// Initialize the backend with a given surface size.
    fn init(&mut self, width: u32, height: u32) -> Result<(), BackendError>;

    /// Resize the rendering surface.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), BackendError>;

    /// Begin a new frame.
    fn begin_frame(&mut self) -> Result<(), BackendError>;

    /// Execute a display list command.
    fn execute(&mut self, command: &DisplayList) -> Result<(), BackendError>;

    /// End the current frame and present to screen.
    fn end_frame(&mut self) -> Result<(), BackendError>;

    /// Get backend capabilities.
    fn capabilities(&self) -> BackendCapabilities;
}

/// Backend capabilities.
#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    /// Maximum texture size.
    pub max_texture_size: u32,
    /// Supports 3D transforms.
    pub supports_3d_transforms: bool,
    /// Supports filters.
    pub supports_filters: bool,
    /// Supports blend modes.
    pub supports_blend_modes: bool,
    /// Supports clip paths.
    pub supports_clip_paths: bool,
}

/// Backend errors.
#[derive(Debug, Clone)]
pub enum BackendError {
    /// Initialization failed.
    InitializationFailed(String),
    /// Surface error.
    SurfaceError(String),
    /// Resource error.
    ResourceError(String),
    /// Rendering error.
    RenderError(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitializationFailed(msg) => write!(f, "Initialization failed: {msg}"),
            Self::SurfaceError(msg) => write!(f, "Surface error: {msg}"),
            Self::ResourceError(msg) => write!(f, "Resource error: {msg}"),
            Self::RenderError(msg) => write!(f, "Render error: {msg}"),
        }
    }
}

impl std::error::Error for BackendError {}
