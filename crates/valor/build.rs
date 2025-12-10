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

/// Collects HTML files from a fixtures directory and adds them to the fixture groups.
fn add_fixtures_to_group(fixtures_path: &Path, fixture_groups: &mut HashMap<String, Vec<PathBuf>>) {
    let crate_name = extract_crate_name(fixtures_path);
    let mut files = Vec::new();
    collect_html_recursively(fixtures_path, &mut files);
    if !files.is_empty() {
        fixture_groups.entry(crate_name).or_default().extend(files);
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
                add_fixtures_to_group(&path, fixture_groups);
            } else if path.file_name().is_some_and(|name| name == "tests") {
                let fixtures_path = path.join("fixtures");
                if fixtures_path.exists() {
                    add_fixtures_to_group(&fixtures_path, fixture_groups);
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
    if let Some(crates_idx) = components.iter().position(|&comp| comp == "crates") {
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
        .skip_while(|&&comp| comp == "fixtures" || comp == "tests")
        .find(|&&comp| !comp.is_empty())
        .map_or_else(|| "unknown".to_string(), |&seg| seg.to_string())
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
    sorted_groups.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));

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
    test_code.push_str("    use super::chromium_compare::fixture_runner;\n");
    test_code.push_str("    use anyhow::Result;\n");
    test_code.push_str("    use std::path::PathBuf;\n\n");

    // Collect all fixtures into one constant
    test_code.push_str("    const ALL_FIXTURES: &[&str] = &[\n");
    for (_crate_name, files) in &sorted_groups {
        for file in files {
            let file_path = file.display().to_string().replace('\\', "\\\\");
            let _ignore = writeln!(test_code, "        r\"{file_path}\",");
        }
    }
    test_code.push_str("    ];\n\n");

    // Generate a single test that runs all fixtures
    test_code.push_str("    /// Tests all fixtures against Chromium.\n");
    test_code.push_str("    ///\n");
    test_code.push_str("    /// # Errors\n");
    test_code.push_str("    ///\n");
    test_code.push_str("    /// Returns an error if the test infrastructure fails.\n");
    test_code.push_str("    ///\n");
    test_code.push_str("    /// # Panics\n");
    test_code.push_str("    ///\n");
    test_code.push_str("    /// May panic if the runtime fails.\n");
    test_code.push_str("    #[tokio::test]\n");
    test_code.push_str("    async fn all_fixtures() -> Result<()> {\n");
    test_code.push_str(
        "        let fixtures: Vec<PathBuf> = ALL_FIXTURES.iter().map(PathBuf::from).collect();\n",
    );
    test_code.push_str("        fixture_runner::run_all_fixtures(&fixtures).await\n");
    test_code.push_str("    }\n");

    test_code.push_str("}\n");

    let _ignore_error = write(&test_file, &test_code);
}
