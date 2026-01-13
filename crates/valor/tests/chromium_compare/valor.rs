use anyhow::Result;
use renderer::{DisplayItem, DisplayList};
use std::env::set_var;
use std::path::Path;
use tokio::runtime::Handle;
use wgpu_backend::{GlyphBounds, PersistentGpuContext, initialize_persistent_context};

use super::layout_tests::setup::setup_page_for_fixture;

/// Creates a new headless GPU context for rendering tests.
/// This uses WGPU's headless rendering, which doesn't require windows or event loops.
/// Tests can now run in parallel without any window creation constraints!
///
/// # Errors
///
/// Returns an error if GPU context creation fails.
pub fn create_render_context(width: u32, height: u32) -> Result<PersistentGpuContext> {
    initialize_persistent_context(width, height)
}

/// Builds BOTH layout JSON and display list from a single HtmlPage to avoid
/// creating two V8 isolates per fixture.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or data extraction fails.
pub async fn build_layout_and_graphics_for_fixture(
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<(serde_json::Value, DisplayList)> {
    use std::time::Instant;
    let start = Instant::now();

    // Set viewport dimensions
    unsafe {
        set_var("VALOR_VIEWPORT_WIDTH", viewport_w.to_string());
        set_var("VALOR_VIEWPORT_HEIGHT", viewport_h.to_string());
    }

    let handle = Handle::current();
    eprintln!("[VALOR_TIMING] setup_page_for_fixture START");
    let mut page = setup_page_for_fixture(&handle, path).await?;
    eprintln!(
        "[VALOR_TIMING] setup_page_for_fixture took: {:?}",
        start.elapsed()
    );

    // setup_page_for_fixture already injected CSS reset and ran layout
    // No need for additional update() calls

    // Extract layout JSON first
    eprintln!("[VALOR_TIMING] serialize_layout START");
    let layout_json = super::layout_tests::serialization::serialize_valor_layout(&mut page)?;
    eprintln!(
        "[VALOR_TIMING] serialize_layout took: {:?}",
        start.elapsed()
    );

    // Then extract display list for graphics
    eprintln!("[VALOR_TIMING] display_list_retained_snapshot START");
    let display_list = page.display_list_retained_snapshot();
    eprintln!(
        "[VALOR_TIMING] display_list_retained_snapshot took: {:?}",
        start.elapsed()
    );

    let clear_color = page.background_rgba();
    let mut items = Vec::with_capacity(display_list.items.len() + 1);
    items.push(DisplayItem::Rect {
        x: 0.0,
        y: 0.0,
        width: viewport_w as f32,
        height: viewport_h as f32,
        color: clear_color,
    });
    items.extend(display_list.items);

    let final_display_list = DisplayList::from_items(items);
    eprintln!(
        "[VALOR_TIMING] build_layout_and_graphics_for_fixture TOTAL: {:?}",
        start.elapsed()
    );

    Ok((layout_json, final_display_list))
}

/// Builds a Valor display list for a given fixture.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or display list generation fails.
pub async fn build_display_list_for_fixture(
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<DisplayList> {
    use super::cache_utils::{CacheFetcher, read_or_fetch_cache};

    // Try to load from cache first
    let cache_key = format!("{}x{}", viewport_w, viewport_h);
    let cached_result = read_or_fetch_cache(CacheFetcher {
        test_name: "display_list",
        fixture_path: path,
        cache_suffix: &format!("_{}.bincode", cache_key),
        fetch_fn: || async { build_display_list_uncached(path, viewport_w, viewport_h).await },
        deserialize_fn: |bytes| bincode::deserialize(bytes).map_err(Into::into),
        serialize_fn: |dl| bincode::serialize(dl).map_err(Into::into),
    })
    .await;

    cached_result
}

async fn build_display_list_uncached(
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<DisplayList> {
    use std::time::Instant;
    let start = Instant::now();

    // Set viewport dimensions via environment for the page
    // Safety: This is a test environment where we control the execution
    unsafe {
        set_var("VALOR_VIEWPORT_WIDTH", viewport_w.to_string());
        set_var("VALOR_VIEWPORT_HEIGHT", viewport_h.to_string());
    }

    let handle = Handle::current();
    eprintln!("[VALOR_TIMING] setup_page_for_fixture START");
    let mut page = setup_page_for_fixture(&handle, path).await?;
    eprintln!(
        "[VALOR_TIMING] setup_page_for_fixture took: {:?}",
        start.elapsed()
    );

    // setup_page_for_fixture already injected CSS reset and ran layout
    // No need for additional update() calls

    eprintln!("[VALOR_TIMING] display_list_retained_snapshot START");
    let display_list = page.display_list_retained_snapshot();
    eprintln!(
        "[VALOR_TIMING] display_list_retained_snapshot took: {:?}",
        start.elapsed()
    );
    let clear_color = page.background_rgba();
    let mut items = Vec::with_capacity(display_list.items.len() + 1);
    items.push(DisplayItem::Rect {
        x: 0.0,
        y: 0.0,
        width: viewport_w as f32,
        height: viewport_h as f32,
        color: clear_color,
    });

    items.extend(display_list.items);
    Ok(DisplayList::from_items(items))
}

type RasterizeResult = (Vec<u8>, Vec<GlyphBounds>);

/// Rasterizes a display list to RGBA bytes using headless GPU rendering.
/// Also returns glyph bounds for text region masking.
///
/// # Errors
///
/// Returns an error if rendering fails.
pub fn rasterize_display_list_to_rgba(
    render_context: &mut PersistentGpuContext,
    display_list: &DisplayList,
    width: u32,
    height: u32,
) -> Result<RasterizeResult> {
    use std::time::Instant;
    let start = Instant::now();

    eprintln!("[VALOR_TIMING] render_display_list_with_context START");
    let rgba = wgpu_backend::render_display_list_with_context(
        render_context,
        display_list,
        width,
        height,
    )?;
    eprintln!(
        "[VALOR_TIMING] render_display_list_with_context took: {:?}",
        start.elapsed()
    );

    // For now, return empty glyph bounds since headless rendering doesn't track them yet
    // TODO: Extend headless rendering to return glyph bounds for text masking
    let glyph_bounds = Vec::new();

    eprintln!(
        "[VALOR_TIMING] rasterize_display_list_to_rgba TOTAL: {:?}",
        start.elapsed()
    );
    Ok((rgba, glyph_bounds))
}
