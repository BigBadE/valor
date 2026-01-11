//! Test setup helpers for layout testing.

use super::super::common::{create_page, css_reset_string, update_until_finished};
use anyhow::{Result, anyhow};
use page_handler::HtmlPage;
use std::path::Path;
use tokio::runtime::Handle;

use super::super::common::to_file_url;

/// Sets up a page for a fixture by creating a page and processing it.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or layout computation fails.
pub async fn setup_page_for_fixture(handle: &Handle, input_path: &Path) -> Result<HtmlPage> {
    let url = to_file_url(input_path)?;
    let mut page = create_page(handle, url).await?;
    page.inject_css_sync(css_reset_string())?;

    let finished = update_until_finished(handle, &mut page, |_page| Ok(())).await?;

    if !finished {
        return Err(anyhow!("Parsing did not finish"));
    }

    // update_until_finished already ran layout in its final iteration
    // No need to call update() or ensure_layout_now() again

    Ok(page)
}
