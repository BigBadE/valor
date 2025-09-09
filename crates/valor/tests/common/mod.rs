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

pub fn fixtures_layout_dir() -> PathBuf {
    fixtures_dir().join("layout")
}

pub fn fixtures_css_dir() -> PathBuf {
    fixtures_dir().join("css")
}

fn collect_html_recursively(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir)
        .map_err(|e| anyhow!("Failed to read dir {}: {}", dir.display(), e))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_dir() {
            collect_html_recursively(&p, out)?;
        } else if p.extension().map(|ext| ext.eq_ignore_ascii_case("html")).unwrap_or(false) {
            out.push(p);
        }
    }
    Ok(())
}

/// Discover all .html files under the tests/fixtures/layout directory recursively.
pub fn fixture_html_files() -> Result<Vec<PathBuf>> {
    let dir = fixtures_layout_dir();
    let mut files: Vec<PathBuf> = Vec::new();
    if dir.exists() {
        collect_html_recursively(&dir, &mut files)?;
    } else {
        // Fallback: scan the legacy top-level fixtures dir non-recursively
        let legacy = fixtures_dir();
        if legacy.exists() {
            let entries = fs::read_dir(&legacy)
                .map_err(|e| anyhow!("Failed to read fixtures dir {}: {}", legacy.display(), e))?;
            files.extend(entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|ext| ext.eq_ignore_ascii_case("html")).unwrap_or(false))
            );
        }
    }
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
    fn extract_id_label(v: &Value) -> Option<String> {
        if let Value::Object(map) = v {
            if let Some(Value::Object(attrs)) = map.get("attrs") {
                if let Some(Value::String(id)) = attrs.get("id") {
                    return Some(format!("#{}", id));
                }
            }
            if let Some(Value::String(id)) = map.get("id") {
                return Some(format!("#{}", id));
            }
        }
        None
    }

    fn is_element_object(v: &Value) -> bool {
        if let Value::Object(map) = v { map.contains_key("tag") && map.contains_key("rect") } else { false }
    }

    fn helper(a: &Value, b: &Value, eps: f64, path: &mut Vec<String>, elem_stack: &mut Vec<(Value, Value)>) -> Result<(), String> {
        use serde_json::Value::*;
        match (a, b) {
            (Null, Null) => Ok(()),
            (Bool(x), Bool(y)) => {
                if x == y { Ok(()) } else { Err(build_err("bool mismatch", &format!("{} != {}", x, y), path, elem_stack)) }
            }
            (Number(x), Number(y)) => {
                match (x.as_f64(), y.as_f64()) {
                    (Some(xf), Some(yf)) => {
                        if (xf - yf).abs() <= eps { Ok(()) } else { Err(build_err("number diff", &format!("{} vs {} exceeds eps {}", xf, yf, eps), path, elem_stack)) }
                    }
                    _ => Err(build_err("non-float number encountered", "", path, elem_stack)),
                }
            }
            (String(xs), String(ys)) => {
                if xs == ys { Ok(()) } else { Err(build_err("string mismatch", &format!("'{}' != '{}'", xs, ys), path, elem_stack)) }
            }
            (Array(xa), Array(ya)) => {
                if xa.len() != ya.len() {
                    return Err(build_err("array length mismatch", &format!("{} != {}", xa.len(), ya.len()), path, elem_stack));
                }
                for (i, (xe, ye)) in xa.iter().zip(ya.iter()).enumerate() {
                    // If this array is a children array, label with element id and compress the path
                    let is_children_ctx = path.last().map(|s| s == ".children").unwrap_or(false);
                    if is_children_ctx {
                        let label = extract_id_label(xe)
                            .or_else(|| extract_id_label(ye))
                            .unwrap_or_else(|| i.to_string());
                        // Replace trailing ".children" with ".<label>" (label is "#id" or numeric)
                        path.pop();
                        path.push(format!(".{}", label));
                    } else {
                        path.push(format!("[{}]", i));
                    }
                    // Maintain element context for better error snippets
                    let pushed = if is_element_object(xe) && is_element_object(ye) {
                        elem_stack.push((xe.clone(), ye.clone()));
                        true
                    } else { false };
                    let r = helper(xe, ye, eps, path, elem_stack);
                    if pushed { elem_stack.pop(); }
                    path.pop();
                    if r.is_err() { return r; }
                }
                Ok(())
            }
            (Object(xo), Object(yo)) => {
                if xo.len() != yo.len() {
                    return Err(build_err("object size mismatch", &format!("{} != {}", xo.len(), yo.len()), path, elem_stack));
                }
                for (k, xv) in xo.iter() {
                    match yo.get(k) {
                        Some(yv) => {
                            path.push(format!(".{}", k));
                            // If descending into an element object, push context
                            let pushed = if is_element_object(xv) && is_element_object(yv) {
                                elem_stack.push((xv.clone(), yv.clone()));
                                true
                            } else { false };
                            let r = helper(xv, yv, eps, path, elem_stack);
                            if pushed { elem_stack.pop(); }
                            path.pop();
                            if r.is_err() { return r; }
                        }
                        None => return Err(build_err("missing key in expected", &format!("'{}'", k), path, elem_stack)),
                    }
                }
                Ok(())
            }
            (x, y) => Err(build_err("type mismatch", &format!("{:?} vs {:?}", type_name(x), type_name(y)), path, elem_stack)),
        }
    }

    fn build_err(kind: &str, detail: &str, path: &Vec<String>, elem_stack: &Vec<(Value, Value)>) -> String {
        let path_str = format_path(path);
        let (our_elem, ch_elem) = if let Some((a, b)) = elem_stack.last() {
            (a, b)
        } else {
            // Fallback: no element context; use empty objects
            (&Value::Null, &Value::Null)
        };
        let our_s = if our_elem.is_null() { String::from("null") } else { serde_json::to_string_pretty(our_elem).unwrap_or_else(|_| String::from("{}")) };
        let ch_s = if ch_elem.is_null() { String::from("null") } else { serde_json::to_string_pretty(ch_elem).unwrap_or_else(|_| String::from("{}")) };
        if detail.is_empty() {
            format!("{}: {}\nElement (our): {}\nElement (chromium): {}", path_str, kind, our_s, ch_s)
        } else {
            format!("{}: {} — {}\nElement (our): {}\nElement (chromium): {}", path_str, kind, detail, our_s, ch_s)
        }
    }

    fn format_path(path: &Vec<String>) -> String {
        if path.is_empty() { String::new() } else {
            let joined = path.join("");
            if joined.starts_with('.') { joined[1..].to_string() } else { joined }
        }
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
    let mut elem_stack: Vec<(Value, Value)> = Vec::new();
    // Seed context with root if it looks like an element
    if is_element_object(actual) && is_element_object(expected) {
        elem_stack.push((actual.clone(), expected.clone()));
    }
    helper(actual, expected, eps, &mut Vec::new(), &mut elem_stack)
}

/// Returns a JS snippet that injects a CSS Reset into the current document in Chromium tests.
/// This should be executed via `tab.evaluate(script, /*return_by_value=*/ false)` after navigation.
pub fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;font-family:monospace,'Courier New',Courier,Consolas,'Liberation Mono',Menlo,Monaco,'DejaVu Sans Mono',monospace;}html,body{margin:0 !important;padding:0 !important;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
            var style = document.createElement('style');
            style.setAttribute('data-valor-test-reset','1');
            style.type = 'text/css';
            style.appendChild(document.createTextNode(css));
            var head = document.head || document.getElementsByTagName('head')[0] || document.documentElement;
            head.appendChild(style);
            // Also enforce via inline styles to ensure immediate override
            var de = document.documentElement; if (de && de.style){ de.style.margin='0'; de.style.padding='0'; de.style.fontFamily='monospace'; }
            var b = document.body; if (b && b.style){ b.style.margin='0'; b.style.padding='0'; b.style.fontFamily='monospace'; }
            // Force style & layout flush
            void (document.body && document.body.offsetWidth);
            return true;
        } catch (e) {
            return false;
        }
    })()"#
}