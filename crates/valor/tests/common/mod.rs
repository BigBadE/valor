#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use url::Url;
use page_handler::state::HtmlPage;
use tokio::runtime::Runtime;
use std::time::Duration;
use serde_json::Value;

/// Returns the directory containing HTML fixtures for integration tests.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Discover all .html files under the tests/fixtures directory.
pub fn fixture_html_files() -> Result<Vec<PathBuf>> {
    let dir = fixtures_dir();
    let entries = fs::read_dir(&dir)
        .map_err(|e| anyhow!("Failed to read fixtures dir {}: {}", dir.display(), e))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|ext| ext.eq_ignore_ascii_case("html")).unwrap_or(false))
        .collect();
    files.sort();
    Ok(files)
}

/// Convert a local file Path to a file:// Url, after canonicalizing when possible.
pub fn to_file_url(p: &Path) -> Result<Url> {
    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    Url::from_file_path(&canonical)
        .map_err(|_| anyhow!("Invalid file path for URL: {}", canonical.display()))
}

/// Construct an HtmlPage using the provided Runtime and Url.
pub fn create_page(rt: &Runtime, url: Url) -> Result<HtmlPage> {
    let page = rt.block_on(HtmlPage::new(rt.handle(), url))?;
    Ok(page)
}

/// Drive page.update() until parsing finishes, running an optional per-tick callback (e.g., drain mirrors).
/// Returns true if finished within the allotted iterations.
pub fn update_until_finished<F>(rt: &Runtime, page: &mut HtmlPage, mut per_tick: F) -> Result<bool>
where
    F: FnMut(&mut HtmlPage) -> Result<()>,
{
    let mut finished = page.parsing_finished();
    for _ in 0..10_000 {
        rt.block_on(page.update())?;
        per_tick(page)?;
        if page.parsing_finished() {
            finished = true;
            break;
        }
        // Yield to background tasks without requiring a Tokio reactor on this thread
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(finished)
}

/// Drive page.update() until parsing finishes (no per-tick callback). Returns true if finished in time.
pub fn update_until_finished_simple(rt: &Runtime, page: &mut HtmlPage) -> Result<bool> {
    update_until_finished(rt, page, |_| Ok(()))
}

/// Compare two serde_json Values with an epsilon for numeric differences.
/// - Floats/integers are compared as f64 within `eps`.
/// - Strings, bools, null are compared for exact equality.
/// - Arrays/Objects must have identical structure and are compared recursively.
/// Returns Ok(()) if equal under epsilon; Err with a path-detailed message otherwise.
pub fn compare_json_with_epsilon(actual: &Value, expected: &Value, eps: f64) -> Result<(), String> {
    fn helper(a: &Value, b: &Value, eps: f64, path: &mut Vec<String>) -> Result<(), String> {
        use serde_json::Value::*;
        match (a, b) {
            (Null, Null) => Ok(()),
            (Bool(x), Bool(y)) => {
                if x == y { Ok(()) } else { Err(format!("{}: bool mismatch: {} != {}", format_path(path), x, y)) }
            }
            (Number(x), Number(y)) => {
                match (x.as_f64(), y.as_f64()) {
                    (Some(xf), Some(yf)) => {
                        if (xf - yf).abs() <= eps { Ok(()) } else { Err(format!("{}: number diff {} vs {} exceeds eps {}", format_path(path), xf, yf, eps)) }
                    }
                    _ => Err(format!("{}: non-float number encountered", format_path(path))),
                }
            }
            (String(xs), String(ys)) => {
                if xs == ys { Ok(()) } else { Err(format!("{}: string mismatch: '{}' != '{}'", format_path(path), xs, ys)) }
            }
            (Array(xa), Array(ya)) => {
                if xa.len() != ya.len() {
                    return Err(format!("{}: array length mismatch: {} != {}", format_path(path), xa.len(), ya.len()));
                }
                for (i, (xe, ye)) in xa.iter().zip(ya.iter()).enumerate() {
                    path.push(format!("[{}]", i));
                    let r = helper(xe, ye, eps, path);
                    path.pop();
                    if r.is_err() { return r; }
                }
                Ok(())
            }
            (Object(xo), Object(yo)) => {
                if xo.len() != yo.len() {
                    return Err(format!("{}: object size mismatch: {} != {}", format_path(path), xo.len(), yo.len()));
                }
                for (k, xv) in xo.iter() {
                    match yo.get(k) {
                        Some(yv) => {
                            path.push(format!(".{}", k));
                            let r = helper(xv, yv, eps, path);
                            path.pop();
                            if r.is_err() { return r; }
                        }
                        None => return Err(format!("{}: missing key '{}' in expected", format_path(path), k)),
                    }
                }
                Ok(())
            }
            (x, y) => Err(format!("{}: type mismatch: {:?} vs {:?}", format_path(path), type_name(x), type_name(y))),
        }
    }
    fn format_path(path: &Vec<String>) -> String {
        if path.is_empty() { "<root>".to_string() } else { path.join("") }
    }
    fn type_name(v: &Value) -> &'static str {
        match v {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }
    helper(actual, expected, eps, &mut Vec::new())
}

/// Returns a JS snippet that injects a CSS Reset into the current document in Chromium tests.
/// This should be executed via `tab.evaluate(script, /*return_by_value=*/ false)` after navigation.
pub fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;}html,body{margin:0 !important;padding:0 !important;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
            var style = document.createElement('style');
            style.setAttribute('data-valor-test-reset','1');
            style.type = 'text/css';
            style.appendChild(document.createTextNode(css));
            var head = document.head || document.getElementsByTagName('head')[0] || document.documentElement;
            head.appendChild(style);
            // Also enforce via inline styles to ensure immediate override
            var de = document.documentElement; if (de && de.style){ de.style.margin='0'; de.style.padding='0'; }
            var b = document.body; if (b && b.style){ b.style.margin='0'; b.style.padding='0'; }
            // Force style & layout flush
            void (document.body && document.body.offsetWidth);
            return true;
        } catch (e) {
            return false;
        }
    })()"#
}