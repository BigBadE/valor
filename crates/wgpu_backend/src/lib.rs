//! Temporary re-export crate to bridge backend users to the new location.
//! This allows dependents to import `wgpu_backend` while we migrate code into `renderer`.

pub use wgpu_renderer::*;
