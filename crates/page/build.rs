use std::env;
use std::fmt::Write as _;
use std::fs::{read_dir, write};
use std::path::{Path, PathBuf};

fn main() {
    generate_fixture_tests();
}

fn collect_html_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "html") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn generate_fixture_tests() {
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let fixtures_dir = PathBuf::from(&manifest_dir).join("tests/fixtures");

    let files = collect_html_files(&fixtures_dir);

    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let test_file = PathBuf::from(out_dir).join("generated_fixture_tests.rs");

    let mut code = String::from("// Auto-generated fixture tests\n\n");
    code.push_str("#[cfg(test)]\n");
    code.push_str("mod tests {\n");
    code.push_str("    use super::*;\n\n");

    // Collect all fixtures into one constant
    code.push_str("    const ALL_FIXTURES: &[&str] = &[\n");
    for file in &files {
        let file_path = file.display().to_string().replace('\\', "\\\\");
        let _ = writeln!(code, "        r\"{file_path}\",");
    }
    code.push_str("    ];\n\n");

    // Generate a single test that runs all fixtures
    code.push_str("    #[test]\n");
    code.push_str("    fn all_fixtures() {\n");
    code.push_str("        run_all_fixtures(ALL_FIXTURES);\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    let _ = write(&test_file, &code);

    println!("cargo:rerun-if-changed={}", fixtures_dir.display());
}
