use crate::core::state::HtmlPage;
use anyhow::Error;
use core::future::Future;
use core::pin::Pin;

/// Type alias for pinned async future returned by JavaScript runtime drivers.
///
/// This reduces type complexity in trait signatures while maintaining
/// lifetime correctness for async operations.
pub type ExecFuture<'future> = Pin<Box<dyn Future<Output = Result<(), Error>> + 'future>>;

/// Trait for JavaScript runtime implementations that execute scripts and timers.
///
/// Implementations of this trait manage the execution of JavaScript code
/// within an `HtmlPage`, including timer callbacks and DOM-driven script execution.
pub trait JsRuntime {
    /// Returns the name of this JavaScript runtime implementation.
    ///
    /// Used for logging and debugging purposes to identify which JS engine
    /// is active (e.g., "v8", "stub", "default").
    ///
    /// Note: Currently unused but reserved for future logging/telemetry.
    fn _name(&self) -> &'static str;

    /// Executes all pending timer callbacks that are ready to fire.
    ///
    /// This method should be called once per event loop iteration to
    /// process `setTimeout` and `setInterval` callbacks.
    fn tick_timers_once(&mut self, page: &mut HtmlPage);

    /// Drives script execution after DOM updates have been applied.
    ///
    /// This async method processes pending scripts and triggers lifecycle
    /// events like `DOMContentLoaded` as needed.
    fn drive_after_dom_update<'driver>(
        &'driver mut self,
        page: &'driver mut HtmlPage,
    ) -> ExecFuture<'driver>;
}

/// Default JavaScript runtime implementation that uses the built-in JS engine.
///
/// This runtime directly invokes the `HtmlPage` methods for timer ticks
/// and script execution without additional abstraction layers.
#[derive(Default)]
pub struct DefaultJsRuntime;

#[cfg(feature = "js")]
impl JsRuntime for DefaultJsRuntime {
    #[inline]
    fn _name(&self) -> &'static str {
        "default"
    }

    #[inline]
    fn tick_timers_once(&mut self, page: &mut HtmlPage) {
        page.tick_js_timers_once();
    }

    #[inline]
    fn drive_after_dom_update<'driver>(
        &'driver mut self,
        page: &'driver mut HtmlPage,
    ) -> ExecFuture<'driver> {
        Box::pin(async move {
            page.execute_pending_scripts();
            page.handle_dom_content_loaded_if_needed()
        })
    }
}

#[cfg(not(feature = "js"))]
impl JsRuntime for DefaultJsRuntime {
    #[inline]
    fn _name(&self) -> &'static str {
        "stub"
    }

    #[inline]
    fn tick_timers_once(&mut self, _page: &mut HtmlPage) {
        // No-op when JS is disabled
    }

    #[inline]
    fn drive_after_dom_update<'driver>(
        &'driver mut self,
        _page: &'driver mut HtmlPage,
    ) -> ExecFuture<'driver> {
        Box::pin(async move { Ok(()) })
    }
}
