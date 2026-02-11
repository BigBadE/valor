//! DOM processing and update helpers.

use anyhow::{Error, anyhow};
use html::parser::HTMLParser;
use js::{DOMMirror, DomIndex, JsEngine};
use log::{info, trace};

/// Finalize DOM loading if the loader has finished.
///
/// # Errors
///
/// Returns an error if DOM finalization fails.
pub(super) async fn finalize_dom_loading_if_needed(
    loader: &mut Option<HTMLParser>,
) -> Result<(), Error> {
    if loader.as_ref().is_some_and(HTMLParser::is_finished) {
        let loader_inst = loader
            .take()
            .ok_or_else(|| anyhow!("Loader is finished and None!"))?;
        trace!("Loader finished, finalizing DOM");
        loader_inst.finish().await?;
    }
    Ok(())
}

/// Handle `DOMContentLoaded` event if needed.
///
/// # Errors
///
/// Returns an error if event handling fails.
pub(super) fn handle_dom_content_loaded_if_needed<E: JsEngine>(
    loader: Option<&HTMLParser>,
    dom_content_loaded_fired: &mut bool,
    js_engine: &mut E,
    dom_index_mirror: &mut DOMMirror<DomIndex>,
) -> Result<(), Error> {
    if loader.is_none() && !*dom_content_loaded_fired {
        info!("HtmlPage: dispatching DOMContentLoaded");
        let _unused = js_engine
            .eval_script(
                "(function(){try{var d=globalThis.document; if(d&&typeof d.__valorDispatchDOMContentLoaded==='function'){ d.__valorDispatchDOMContentLoaded(); }}catch(_){}})();",
                "valor://dom_events",
            );
        let _unused2 = js_engine.run_jobs();
        *dom_content_loaded_fired = true;
        // After DOMContentLoaded, DOM listener mutations will be applied on the next regular tick.
        // Keep the DOM index mirror in sync in a non-blocking manner for tests.
        dom_index_mirror.try_update_sync()?;
    }
    Ok(())
}
