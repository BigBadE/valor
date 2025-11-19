use std::env;
use std::fmt::Write as _;
use std::fs::{read_dir, write};
use std::path::{Path, PathBuf};

fn main() {
    // Tell cargo to rerun if fixtures change
    println!("cargo:rerun-if-changed=tests/fixtures");

    // Discover CSS module fixtures
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let workspace_root = PathBuf::from(&manifest_dir).join("..").join("..");

    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates/css/modules").display()
    );

    // Generate fixture test code
    generate_fixture_tests();
}

fn collect_html_recursively(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_html_recursively(&path, out);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
            {
                out.push(path);
            }
        }
    }
}

/// Returns directories containing fixture files.
///
/// # Panics
///
/// Panics if `CARGO_MANIFEST_DIR` is not set (which should never happen during build).
fn fixture_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let Ok(manifest_dir_str) = env::var("CARGO_MANIFEST_DIR") else {
        return roots;
    };
    let manifest_dir = PathBuf::from(manifest_dir_str);
    let workspace_root = manifest_dir.join("..").join("..");

    // Local fixtures
    let local = manifest_dir.join("tests").join("fixtures").join("layout");
    if local.exists() {
        roots.push(local);
    }

    // CSS module fixtures
    let modules_dir = workspace_root.join("crates").join("css").join("modules");
    if let Ok(entries) = read_dir(&modules_dir) {
        for entry in entries.filter_map(Result::ok) {
            let fixture_path = entry.path().join("tests").join("fixtures");
            if fixture_path.exists() {
                roots.push(fixture_path);
            }
        }
    }

    roots
}

fn sanitize_name(path: &Path) -> String {
    // Create a valid Rust identifier from a file path
    let name = path.to_string_lossy();
    let mut result = String::new();

    for character in name.chars() {
        if character.is_alphanumeric() {
            result.push(character);
        } else {
            result.push('_');
        }
    }

    // Remove consecutive underscores and trim
    while result.contains("__") {
        result = result.replace("__", "_");
    }

    result.trim_matches('_').to_lowercase()
}

/// Generates individual test functions for each fixture file.
///
/// # Panics
///
/// Panics if `OUT_DIR` is not set or if writing the generated file fails.
fn generate_fixture_tests() {
    let mut files = Vec::new();
    for root in fixture_roots() {
        collect_html_recursively(&root, &mut files);
    }

    // Filter to only include files in proper fixture subdirectories
    files.retain(|path| {
        let parent_not_fixtures = path
            .parent()
            .and_then(|dir| dir.file_name())
            .is_some_and(|name| name != "fixtures");

        let mut has_fixtures_ancestor = false;
        for anc in path.ancestors().skip(1) {
            if let Some(name) = anc.file_name()
                && name == "fixtures"
            {
                has_fixtures_ancestor = true;
                break;
            }
        }

        has_fixtures_ancestor && parent_not_fixtures
    });

    // Sort for deterministic output
    files.sort();

    // Generate test code
    let Ok(out_dir_str) = env::var("OUT_DIR") else {
        return;
    };
    let out_dir = PathBuf::from(out_dir_str);
    let test_file = out_dir.join("generated_fixture_tests.rs");

    let mut test_code = String::from("// Auto-generated fixture tests\n\n");
    test_code.push_str("#[cfg(test)]\nmod generated_fixture_tests {\n");
    test_code.push_str("    use super::chromium_compare::{common, layout_tests};\n");
    test_code.push_str("    use anyhow::Result;\n");
    test_code.push_str("    use std::path::PathBuf;\n\n");

    for (idx, file) in files.iter().enumerate() {
        let test_name = format!("fixture_{}_{}", idx, sanitize_name(file));
        let file_path = file.display().to_string().replace('\\', "\\\\");

        test_code.push_str("    /// Tests layout computation against Chromium for this fixture.\n");
        test_code.push_str("    ///\n");
        test_code.push_str("    /// # Errors\n");
        test_code.push_str("    ///\n");
        test_code.push_str("    /// Returns an error if layout computation or comparison fails.\n");
        test_code.push_str("    #[tokio::test(flavor = \"multi_thread\", worker_threads = 2)]\n");
        let _ignore1 = writeln!(test_code, "    async fn {test_name}() -> Result<()> {{");
        test_code.push_str("        common::init_test_logger();\n");
        let _ignore2 = writeln!(
            test_code,
            "        let path = PathBuf::from(r\"{file_path}\");"
        );
        test_code.push_str("        layout_tests::run_single_layout_test(&path).await\n");
        test_code.push_str("    }\n\n");
    }

    test_code.push_str("}\n");

    if let Err(_err) = write(&test_file, &test_code) {
        // Silently ignore write errors - build will continue anyway
    }
}
