//! Build script for `js_engine_v8`.
//!
//! Ensures required Windows system libraries are linked when building with
//! MSVC. The `v8` crate uses ETW (Event Tracing for Windows) and Windows
//! Registry APIs which live in `advapi32`.
fn main() {
    // Ensure required Windows system libraries are linked when building with MSVC.
    // The v8 crate uses ETW (Event Tracing for Windows) and Windows Registry APIs which live in advapi32.
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
