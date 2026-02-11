//! GPU context pool for efficient parallel test execution.
//!
//! Instead of creating a new GPU context for each test (expensive),
//! we maintain a pool of contexts that can be reused across tests.

use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use wgpu_backend::offscreen::PersistentGpuContext;

/// Global pool of GPU contexts
static GPU_POOL: Lazy<Mutex<Vec<PersistentGpuContext>>> = Lazy::new(|| {
    let num_contexts = num_cpus::get();
    eprintln!("=== INITIALIZING GPU POOL: {} contexts ===", num_contexts);

    let contexts: Vec<_> = (0..num_contexts)
        .filter_map(
            |i| match wgpu_backend::offscreen::initialize_persistent_context(800, 600) {
                Ok(ctx) => {
                    eprintln!("  [{}] GPU context initialized", i);
                    Some(ctx)
                }
                Err(e) => {
                    eprintln!("  [{}] Failed to initialize GPU context: {}", i, e);
                    None
                }
            },
        )
        .collect();

    eprintln!(
        "=== GPU POOL READY: {}/{} contexts ===",
        contexts.len(),
        num_contexts
    );
    Mutex::new(contexts)
});

/// Borrows a GPU context from the pool.
/// Blocks if all contexts are currently in use.
pub struct GpuContextGuard {
    context: Option<PersistentGpuContext>,
}

impl GpuContextGuard {
    /// Get a mutable reference to the GPU context
    pub fn get_mut(&mut self) -> &mut PersistentGpuContext {
        self.context.as_mut().expect("Context should be present")
    }
}

impl Drop for GpuContextGuard {
    fn drop(&mut self) {
        // Return context to pool
        if let Some(context) = self.context.take() {
            GPU_POOL
                .lock()
                .expect("Failed to lock GPU pool")
                .push(context);
        }
    }
}

/// Acquire a GPU context from the pool.
/// Blocks until a context becomes available.
pub fn acquire_gpu_context() -> Result<GpuContextGuard> {
    loop {
        let mut pool = GPU_POOL.lock().expect("Failed to lock GPU pool");
        if let Some(context) = pool.pop() {
            drop(pool); // Release lock
            return Ok(GpuContextGuard {
                context: Some(context),
            });
        }
        drop(pool);

        // Wait a bit before retrying
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
