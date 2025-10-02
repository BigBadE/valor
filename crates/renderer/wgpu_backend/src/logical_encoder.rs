use anyhow::Result as AnyResult;
use std::sync::Arc;
use wgpu::{CommandEncoder, CommandEncoderDescriptor, Device, Queue};

use crate::error::submit_with_validation;

/// Logical encoder that manages multiple real encoders for proper D3D12 resource transitions.
///
/// D3D12 requires resource state transitions (`RENDER_TARGET` → `PIXEL_SHADER_RESOURCE`) to happen
/// between command buffer submissions. This abstraction provides a single logical encoder interface
/// while managing multiple real encoders underneath, submitting command buffers at strategic points.
///
/// Architecture:
/// - One "logical" encoder from the caller's perspective
/// - Multiple "real" encoders created and submitted as needed
/// - Automatic submission before texture usage to ensure proper state transitions
/// - Resource lifetime management across submissions
pub struct LogicalEncoder {
    /// The current command encoder, if one exists.
    current_encoder: Option<CommandEncoder>,
    /// Counter for generating unique encoder labels.
    label_counter: u32,
}

impl LogicalEncoder {
    /// Create a new logical encoder.
    pub const fn new() -> Self {
        Self {
            current_encoder: None,
            label_counter: 0,
        }
    }

    /// Get or create the current encoder.
    fn ensure_encoder(&mut self, device: &Arc<Device>) -> &mut CommandEncoder {
        self.current_encoder.get_or_insert_with(|| {
            self.label_counter += 1;
            device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some(&format!("logical-encoder-{}", self.label_counter)),
            })
        })
    }

    /// Submit the current encoder and immediately create a new one.
    /// This is critical for D3D12 resource state transitions (`RENDER_TARGET` → `PIXEL_SHADER_RESOURCE`).
    /// Use this after offscreen rendering completes and before the main pass that will sample those textures.
    ///
    /// # Errors
    /// Returns an error if command buffer submission fails.
    pub fn submit_and_renew(&mut self, device: &Arc<Device>, queue: &Queue) -> AnyResult<()> {
        let old_counter = self.label_counter;
        // Submit current encoder if it exists
        if let Some(encoder) = self.current_encoder.take() {
            log::debug!(target: "wgpu_renderer", ">>> submit_and_renew: finishing encoder #{old_counter}");
            let command_buffer = encoder.finish();
            log::debug!(target: "wgpu_renderer", ">>> submit_and_renew: submitting command buffer");
            submit_with_validation(device, queue, [command_buffer])?;
            log::debug!(target: "wgpu_renderer", ">>> submit_and_renew: submission successful");
        } else {
            log::warn!(target: "wgpu_renderer", ">>> submit_and_renew: no encoder to submit!");
        }
        // Immediately create a new encoder for subsequent work
        self.label_counter += 1;
        log::debug!(target: "wgpu_renderer", ">>> submit_and_renew: creating new encoder #{}", self.label_counter);
        let new_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some(&format!("logical-encoder-{}", self.label_counter)),
        });
        self.current_encoder = Some(new_encoder);
        log::debug!(target: "wgpu_renderer", ">>> submit_and_renew: new encoder created successfully");
        Ok(())
    }

    /// Submit the current encoder without creating a new one.
    /// Only use this for final cleanup.
    ///
    /// # Errors
    /// Returns an error if command buffer submission fails.
    fn submit_current(&mut self, device: &Arc<Device>, queue: &Queue) -> AnyResult<()> {
        if let Some(encoder) = self.current_encoder.take() {
            let command_buffer = encoder.finish();
            submit_with_validation(device, queue, [command_buffer])?;
        }
        Ok(())
    }

    /// Get a mutable reference to the current encoder.
    /// Use this to access encoder methods directly (`begin_render_pass`, `push_debug_group`, etc.)
    pub fn encoder(&mut self, device: &Arc<Device>) -> &mut CommandEncoder {
        if self.current_encoder.is_none() {
            log::debug!(target: "wgpu_renderer", ">>> encoder(): creating initial encoder");
        }
        self.ensure_encoder(device)
    }

    /// Finish all encoders and submit all remaining command buffers.
    ///
    /// # Errors
    /// Returns an error if command buffer submission fails.
    pub fn finish_and_submit(mut self, device: &Arc<Device>, queue: &Queue) -> AnyResult<()> {
        log::debug!(target: "wgpu_renderer", ">>> finish_and_submit: encoder exists = {}", self.current_encoder.is_some());
        self.submit_current(device, queue)?;
        log::debug!(target: "wgpu_renderer", ">>> finish_and_submit: completed successfully");
        Ok(())
    }
}

impl Default for LogicalEncoder {
    fn default() -> Self {
        Self::new()
    }
}
