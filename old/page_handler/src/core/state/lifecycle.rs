//! Page lifecycle management.

use super::HtmlPage;
use anyhow::Error;

impl HtmlPage {
    /// Returns true once parsing has fully finalized and the loader has been consumed.
    /// This becomes true only after an `update()` call has observed the parser finished
    /// and awaited its completion.
    pub const fn parsing_finished(&self) -> bool {
        self.loader.is_none()
    }

    /// Handle `DOMContentLoaded` event if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if event handling fails.
    #[cfg(feature = "js")]
    pub(crate) fn handle_dom_content_loaded_if_needed(&mut self) -> Result<(), Error> {
        if let Some(js_engine) = self.js_engine.as_mut() {
            super::dom_processing::handle_dom_content_loaded_if_needed(
                self.loader.as_ref(),
                &mut self.lifecycle.dom_content_loaded_fired,
                js_engine,
                &mut self.dom_index_mirror,
            )
        } else {
            Ok(())
        }
    }
}
