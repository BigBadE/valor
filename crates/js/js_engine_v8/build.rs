fn main() {
    // Ensure required Windows system libraries are linked when building with MSVC + rust-lld.
    // rusty_v8 uses ETW (Event Tracing for Windows) and Windows Registry APIs which live in advapi32.
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
