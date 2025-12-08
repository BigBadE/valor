use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::fs::{read_dir, write};
use std::path::{Path, PathBuf};

fn main() {
    // Tell cargo to rerun if crates directory changes
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let workspace_root = PathBuf::from(&manifest_dir).join("..").join("..");
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates").display()
    );

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

/// Recursively scans a directory for crate fixture folders and groups fixtures by crate.
fn scan_crate_fixtures(dir: &Path, fixture_groups: &mut HashMap<String, Vec<PathBuf>>) {
    if !dir.exists() {
        return;
    }

    let Ok(entries) = read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "fixtures") {
                // Found a fixtures directory - determine the crate name
                let crate_name = extract_crate_name(&path);
                let mut files = Vec::new();
                collect_html_recursively(&path, &mut files);
                if !files.is_empty() {
                    fixture_groups.entry(crate_name).or_default().extend(files);
                }
            } else if path.file_name().is_some_and(|name| name == "tests") {
                let fixtures_path = path.join("fixtures");
                if fixtures_path.exists() {
                    let crate_name = extract_crate_name(&fixtures_path);
                    let mut files = Vec::new();
                    collect_html_recursively(&fixtures_path, &mut files);
                    if !files.is_empty() {
                        fixture_groups.entry(crate_name).or_default().extend(files);
                    }
                }
            } else {
                scan_crate_fixtures(&path, fixture_groups);
            }
        }
    }
}

/// Extracts a meaningful crate name from the fixture path.
fn extract_crate_name(fixtures_path: &Path) -> String {
    // Try to extract crate name from path like "crates/css/modules/flexbox/tests/fixtures"
    let path_str = fixtures_path.display().to_string();
    let components: Vec<&str> = path_str.split(['/', '\\']).collect();

    // Find the index of "crates" and build the name from there
    if let Some(crates_idx) = components.iter().position(|&c| c == "crates") {
        let relevant_parts: Vec<&str> = components[crates_idx + 1..]
            .iter()
            .filter(|&&part| part != "tests" && part != "fixtures" && !part.is_empty())
            .copied()
            .collect();
        if !relevant_parts.is_empty() {
            return relevant_parts.join("_");
        }
    }

    // Fallback to last non-empty directory before fixtures
    components
        .iter()
        .rev()
        .skip_while(|&&c| c == "fixtures" || c == "tests")
        .find(|&&c| !c.is_empty())
        .map_or_else(|| "unknown".to_string(), |&s| s.to_string())
}

/// Converts a name to a valid Rust identifier.
fn sanitize_test_name(name: &str) -> String {
    let mut result = String::new();
    let mut prev_was_underscore = false;

    for c in name.chars() {
        if c.is_alphanumeric() {
            result.push(c);
            prev_was_underscore = false;
        } else if !prev_was_underscore {
            result.push('_');
            prev_was_underscore = true;
        }
    }

    // Trim leading/trailing underscores
    result.trim_matches('_').to_string()
}

fn generate_fixture_tests() {
    let Ok(manifest_dir_str) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let workspace_root = PathBuf::from(manifest_dir_str).join("..").join("..");
    let crates_dir = workspace_root.join("crates");

    let mut fixture_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    scan_crate_fixtures(&crates_dir, &mut fixture_groups);

    // Sort fixture groups by name for deterministic output
    let mut sorted_groups: Vec<_> = fixture_groups.into_iter().collect();
    sorted_groups.sort_by(|a, b| a.0.cmp(&b.0));

    // Sort files within each group
    for (_name, files) in &mut sorted_groups {
        files.sort();
    }

    let Ok(out_dir_str) = env::var("OUT_DIR") else {
        return;
    };
    let out_dir = PathBuf::from(out_dir_str);
    let test_file = out_dir.join("generated_fixture_tests.rs");

    let mut test_code = String::from("// Auto-generated fixture tests\n\n");
    test_code.push_str("#[cfg(test)]\nmod tests {\n");
    test_code.push_str("    use super::chromium_compare::common;\n");
    test_code.push_str("    use anyhow::Result;\n");
    test_code.push_str("    use std::path::PathBuf;\n\n");

    // Generate a separate test for each crate's fixtures
    for (crate_name, files) in &sorted_groups {
        let test_name = sanitize_test_name(crate_name);

        // Generate constant for this group's fixture paths
        let const_name = format!("FIXTURES_{}", test_name.to_uppercase());
        test_code.push_str(&format!("    const {const_name}: &[&str] = &[\n"));
        for file in files {
            let file_path = file.display().to_string().replace('\\', "\\\\");
            let _ignore = writeln!(test_code, "        r\"{file_path}\",");
        }
        test_code.push_str("    ];\n\n");

        // Generate test function for this group
        test_code.push_str(&format!(
            "    /// Tests fixtures from {crate_name} crate against Chromium.\n"
        ));
        test_code.push_str("    ///\n");
        test_code.push_str("    /// # Errors\n");
        test_code.push_str("    ///\n");
        test_code.push_str("    /// Returns an error if the test infrastructure fails.\n");
        test_code.push_str("    ///\n");
        test_code.push_str("    /// # Panics\n");
        test_code.push_str("    ///\n");
        test_code.push_str("    /// May panic if the runtime fails.\n");
        test_code.push_str("    #[tokio::test]\n");
        test_code.push_str(&format!("    async fn {test_name}() -> Result<()> {{\n"));
        test_code.push_str(&format!(
            "        let fixtures: Vec<PathBuf> = {const_name}.iter().map(PathBuf::from).collect();\n"
        ));
        test_code.push_str("        common::run_all_fixtures(&fixtures).await\n");
        test_code.push_str("    }\n\n");
    }

    test_code.push_str("}\n");

    let _ignore_error = write(&test_file, &test_code);
}
