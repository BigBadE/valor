//! WGPU error scope management for validation tracking.

use anyhow::{Result as AnyResult, anyhow};
use pollster::block_on;
use std::sync::Arc;
use wgpu::{Device, ErrorFilter};

/// RAII guard for WGPU error scopes.
/// Automatically pushes an error scope on creation and pops it on drop.
/// CRITICAL: Must call `check()` before dropping to avoid error scope imbalance.
pub struct ErrorScopeGuard {
    /// GPU device for error scope management.
    device: Arc<Device>,
    /// Label for debugging error scopes.
    label: &'static str,
    /// Whether `check()` has been called.
    checked: bool,
}

impl ErrorScopeGuard {
    /// Push an error scope and return a guard that will pop it on drop.
    pub(crate) fn push(device: &Arc<Device>, label: &'static str) -> Self {
        device.push_error_scope(ErrorFilter::Validation);
        Self {
            device: Arc::clone(device),
            label,
            checked: false,
        }
    }

    /// Check for errors in this scope. Must be called before dropping.
    /// This is now foolproof - it sets the checked flag and delegates to `do_check`.
    ///
    /// # Errors
    /// Returns an error if a WGPU validation error is detected.
    pub(crate) fn check(mut self) -> AnyResult<()> {
        self.checked = true;
        self.do_check()
    }

    /// Check for WGPU errors by polling the error scope.
    ///
    /// # Errors
    /// Returns an error if a WGPU validation error is detected.
    fn do_check(&self) -> AnyResult<()> {
        let fut = self.device.pop_error_scope();
        let res = block_on(fut);
        if let Some(err) = res {
            return Err(anyhow!(
                "wgpu validation error in scope '{}': {err:?}",
                self.label
            ));
        }
        Ok(())
    }
}

impl Drop for ErrorScopeGuard {
    fn drop(&mut self) {
        if !self.checked {
            // CRITICAL: If check() wasn't called, this is a bug that will cause error scope imbalance
            // Pop the scope anyway to prevent imbalance
            drop(self.do_check());
        }
    }
}
