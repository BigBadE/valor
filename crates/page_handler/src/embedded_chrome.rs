//! Embedded chrome assets for the `valor://` URL scheme.
//!
//! This module maps known chrome asset paths to embedded byte slices using `include_bytes!`.
//! Paths are resolved relative to the repository's `assets/chrome` directory.
//! Only a minimal set is embedded for Phase 1 bootstrap.

/// Returns embedded bytes for a given valor chrome asset path.
///
/// Supported path forms:
/// - Full URL: `"valor://chrome/index.html"`
/// - Path after authority: `"/index.html"`
/// - Bare filename: `"index.html"`
///
/// # Arguments
///
/// * `path` - The asset path in any of the supported forms
///
/// # Returns
///
/// `Some(&'static [u8])` if the asset is found, `None` otherwise.
#[must_use]
pub fn get_embedded_chrome_asset(path: &str) -> Option<&'static [u8]> {
    // Normalize the path by stripping scheme and authority if present.
    let normalized = normalize_path(path);
    match normalized.as_str() {
        "index.html" | "/index.html" => Some(include_bytes!("../../../assets/chrome/index.html")),
        "app.js" | "/app.js" => Some(include_bytes!("../../../assets/chrome/app.js")),
        _other => None,
    }
}

/// Normalizes various valor chrome path inputs to a canonical form.
///
/// Strips the `valor://chrome/` or `valor://chrome` prefix if present,
/// leaving just the path portion.
#[must_use]
fn normalize_path(input: &str) -> String {
    // If the full valor URL is provided, strip the scheme/host prefix.
    let trimmed = input
        .strip_prefix("valor://chrome/")
        .or_else(|| input.strip_prefix("valor://chrome"))
        .unwrap_or(input);
    // Ensure we have either "index.html" or "/index.html"-style outputs.
    trimmed.to_owned()
}
