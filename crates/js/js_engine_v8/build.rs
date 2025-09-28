//! Build script for `js_engine_v8`.
//!
//! Ensures required Windows system libraries are linked when building with
//! MSVC + rust-lld. `rusty_v8` uses ETW (Event Tracing for Windows) and
//! Windows Registry APIs which live in `advapi32`.
fn main() {
    // Ensure required Windows system libraries are linked when building with MSVC + rust-lld.
    // rusty_v8 uses ETW (Event Tracing for Windows) and Windows Registry APIs which live in advapi32.
    // Only needed when the real V8 engine is enabled.
    #[cfg(all(target_os = "windows", feature = "v8"))]
    {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
