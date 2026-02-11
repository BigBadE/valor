use serde_json::Value as JsonValue;
use std::fs;
use std::path::{Path, PathBuf};

fn cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("test_cache")
        .join("layout")
}

fn failing_dir() -> PathBuf {
    cache_dir().join("failing")
}

/// FNV-1a hash of a path string for cache keys.
fn fixture_hash(path: &Path) -> u64 {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let bytes = canonical.to_string_lossy().as_bytes().to_vec();

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Read cached Chrome JSON for a fixture.
pub fn read_cached(path: &Path) -> Option<JsonValue> {
    let hash = fixture_hash(path);
    let cache_path = cache_dir().join(format!("{hash}_chrome.json"));
    let data = fs::read_to_string(cache_path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Write Chrome JSON to cache.
pub fn write_cache(path: &Path, json: &JsonValue) {
    let hash = fixture_hash(path);
    let dir = cache_dir();
    let _ = fs::create_dir_all(&dir);
    let cache_path = dir.join(format!("{hash}_chrome.json"));
    if let Ok(data) = serde_json::to_string_pretty(json) {
        let _ = fs::write(cache_path, data);
    }
}

/// Write failure artifacts for debugging.
pub fn write_failure_artifacts(
    fixture_name: &str,
    chrome: &JsonValue,
    valor: &JsonValue,
    error: &str,
) {
    let dir = failing_dir();
    let _ = fs::create_dir_all(&dir);

    if let Ok(data) = serde_json::to_string_pretty(chrome) {
        let _ = fs::write(dir.join(format!("{fixture_name}.chrome.json")), data);
    }
    if let Ok(data) = serde_json::to_string_pretty(valor) {
        let _ = fs::write(dir.join(format!("{fixture_name}.valor.json")), data);
    }
    let _ = fs::write(dir.join(format!("{fixture_name}.error.txt")), error);
}
