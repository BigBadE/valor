use anyhow::{Result as RendererResult, anyhow};
use pollster::block_on;
use wgpu::{CommandBuffer, Device, ErrorFilter, Queue};

/// Submit command buffers with a validation error scope and map any errors into anyhow with context.
///
/// # Errors
/// Returns an error if WGPU validation fails during command buffer submission.
pub fn submit_with_validation<I>(
    device: &Device,
    queue: &Queue,
    submissions: I,
) -> RendererResult<()>
where
    I: IntoIterator<Item = CommandBuffer>,
{
    log::debug!(target: "wgpu_renderer", ">>> submit_with_validation: pushing error scope");
    device.push_error_scope(ErrorFilter::Validation);
    log::debug!(target: "wgpu_renderer", ">>> submit_with_validation: submitting command buffers");
    queue.submit(submissions);
    log::debug!(target: "wgpu_renderer", ">>> submit_with_validation: popping error scope");
    let fut = device.pop_error_scope();
    let res = block_on(fut);
    if let Some(err) = res {
        log::error!(target: "wgpu_renderer", "WGPU error (scoped submit): {err:?}");
        return Err(anyhow!("wgpu scoped error on submit: {err:?}"));
    }
    log::debug!(target: "wgpu_renderer", ">>> submit_with_validation: success");
    Ok(())
}

/// Run a closure while a validation error scope is active. Useful to pinpoint failing API calls.
///
/// # Errors
/// Returns an error if WGPU validation fails within the validation scope.
pub fn with_validation_scope<F, T>(device: &Device, label: &str, f: F) -> RendererResult<T>
where
    F: FnOnce() -> T,
{
    device.push_error_scope(ErrorFilter::Validation);
    let out = f();
    let fut = device.pop_error_scope();
    let res = block_on(fut);
    if let Some(err) = res {
        log::error!(target: "wgpu_renderer", "WGPU error in scope '{label}': {err:?}");
        return Err(anyhow!("wgpu scoped error in {label}: {err:?}"));
    }
    Ok(out)
}
