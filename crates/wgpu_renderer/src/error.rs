use anyhow::{Result as RendererResult, anyhow};
use wgpu::{CommandBuffer, Device, ErrorFilter, Queue};

/// Submit command buffers with a validation error scope and map any errors into anyhow with context.
pub fn submit_with_validation<I>(
    device: &Device,
    queue: &Queue,
    submissions: I,
) -> RendererResult<()>
where
    I: IntoIterator<Item = CommandBuffer>,
{
    device.push_error_scope(ErrorFilter::Validation);
    queue.submit(submissions);
    let fut = device.pop_error_scope();
    let res = pollster::block_on(fut);
    if let Some(err) = res {
        log::error!(target: "wgpu_renderer", "WGPU error (scoped submit): {err:?}");
        return Err(anyhow!("wgpu scoped error on submit: {err:?}"));
    }
    Ok(())
}

/// Run a closure while a validation error scope is active. Useful to pinpoint failing API calls.
pub fn with_validation_scope<F, T>(device: &Device, label: &str, f: F) -> RendererResult<T>
where
    F: FnOnce() -> T,
{
    device.push_error_scope(ErrorFilter::Validation);
    let out = f();
    let fut = device.pop_error_scope();
    let res = pollster::block_on(fut);
    if let Some(err) = res {
        log::error!(target: "wgpu_renderer", "WGPU error in scope '{label}': {err:?}");
        return Err(anyhow!("wgpu scoped error in {label}: {err:?}"));
    }
    Ok(out)
}
