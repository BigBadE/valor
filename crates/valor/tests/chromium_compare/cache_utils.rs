// Cache utilities for test results
// Used by graphics_tests and layout_tests to cache Chrome screenshots and layout data

use anyhow::Result;
use std::fs::{read, write};
use std::future::Future;
use std::path::{Path, PathBuf};

use super::common::test_cache_dir;

// ===== FNV-1a hash for cache keys =====

const fn checksum_u64(input_str: &str) -> u64 {
    let bytes = input_str.as_bytes();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
        index += 1;
    }
    hash
}

/// Generates a cache file path for a fixture test result.
///
/// # Errors
///
/// Returns an error if directory creation fails.
pub fn cache_file_path(test_name: &str, fixture_path: &Path, suffix: &str) -> Result<PathBuf> {
    let dir = test_cache_dir(test_name)?;
    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let path_str = canon.to_string_lossy();
    let hash = checksum_u64(&path_str);
    let cache_file = dir.join(format!("{hash:016x}{suffix}"));
    Ok(cache_file)
}

/// Checks if a cache entry exists for the given fixture and test.
///
/// # Errors
///
/// Returns an error if directory creation fails.
pub fn cache_exists(test_name: &str, fixture_path: &Path, suffix: &str) -> Result<bool> {
    let cache_path = cache_file_path(test_name, fixture_path, suffix)?;
    Ok(cache_path.exists())
}

type DeserializeFn<T> = fn(&[u8]) -> Result<T>;
type SerializeFn<T> = fn(&T) -> Result<Vec<u8>>;

pub struct CacheFetcher<'cache, T, F> {
    pub test_name: &'cache str,
    pub fixture_path: &'cache Path,
    pub cache_suffix: &'cache str,
    pub fetch_fn: F,
    pub deserialize_fn: DeserializeFn<T>,
    pub serialize_fn: SerializeFn<T>,
}

/// Reads from cache or fetches and caches the result.
///
/// # Errors
///
/// Returns an error if fetching or deserializing fails.
pub async fn read_or_fetch_cache<T, F, Fut>(fetcher: CacheFetcher<'_, T, F>) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let cache_path = cache_file_path(
        fetcher.test_name,
        fetcher.fixture_path,
        fetcher.cache_suffix,
    )?;

    // Try to read from cache
    if let Ok(bytes) = read(&cache_path)
        && let Ok(value) = (fetcher.deserialize_fn)(&bytes)
    {
        return Ok(value);
    }

    // Fetch fresh value
    let value = (fetcher.fetch_fn)().await?;

    // Write to cache
    let bytes = (fetcher.serialize_fn)(&value)?;
    let _ignore_write_error = write(&cache_path, &bytes);

    Ok(value)
}
