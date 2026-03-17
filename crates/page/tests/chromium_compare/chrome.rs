use anyhow::{Result, anyhow};
use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::fetcher::{BrowserFetcher, BrowserFetcherOptions};
use chromiumoxide::page::Page;
use futures_util::StreamExt;
use serde_json::{Value as JsonValue, from_str};
use std::io::Read as _;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::spawn;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use super::cache;
use super::common;

const CHROME_PORT: u16 = 19222;

/// Browser handle with its event handler task and Chrome process.
pub struct BrowserWithHandler {
    pub browser: Browser,
    _handler_task: JoinHandle<()>,
    chrome_process: Option<Child>,
    user_data_dir: Option<PathBuf>,
}

impl Drop for BrowserWithHandler {
    fn drop(&mut self) {
        if let Some(mut process) = self.chrome_process.take() {
            let _ = process.kill();
        }
        if let Some(dir) = self.user_data_dir.take() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// Chrome is always available — the fetcher will download it if needed.
pub fn chrome_available() -> bool {
    true
}

fn find_chrome_local() -> Option<PathBuf> {
    for candidate in ["google-chrome", "chromium", "chromium-browser"] {
        if let Ok(output) = Command::new(candidate).arg("--version").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("Chrome") || stdout.contains("Chromium") {
                return Some(PathBuf::from(candidate));
            }
        }
    }
    None
}

/// Download Chrome via the chromiumoxide fetcher and return the binary path.
async fn fetch_chrome() -> Result<PathBuf> {
    // Ensure the default cache directory exists before the fetcher tries to write.
    if let Some(home) = std::env::var_os("HOME") {
        let cache_dir = PathBuf::from(home).join(".cache").join("chromiumoxide");
        let _ = std::fs::create_dir_all(&cache_dir);
    }
    let options = BrowserFetcherOptions::default()
        .map_err(|err| anyhow!("BrowserFetcher options: {err}"))?;
    let fetcher = BrowserFetcher::new(options);
    let info = fetcher.fetch().await.map_err(|err| {
        let mut msg = format!("BrowserFetcher failed: {err}");
        let mut source: Option<&dyn std::error::Error> = std::error::Error::source(&err);
        while let Some(cause) = source {
            msg.push_str(&format!("\n  caused by: {cause}"));
            source = std::error::Error::source(cause);
        }
        anyhow!("{msg}")
    })?;
    eprintln!("  Downloaded to: {}", info.executable_path.display());
    Ok(info.executable_path)
}

async fn find_chrome() -> Result<PathBuf> {
    if let Some(local) = find_chrome_local() {
        return Ok(local);
    }
    eprintln!("Chrome not found locally, downloading via fetcher...");
    fetch_chrome().await
}

fn is_chrome_running() -> bool {
    TcpStream::connect(format!("127.0.0.1:{CHROME_PORT}")).is_ok()
}

async fn kill_existing_chrome() {
    let _ = Command::new("pkill").args(["-9", "-f", "chrome"]).output();
    sleep(Duration::from_secs(1)).await;
}

fn chrome_args(user_data_dir: &Path) -> Vec<String> {
    vec![
        format!("--remote-debugging-port={CHROME_PORT}"),
        format!("--user-data-dir={}", user_data_dir.display()),
        "--headless=new".to_string(),
        "--disable-gpu".to_string(),
        "--no-sandbox".to_string(),
        "--disable-dev-shm-usage".to_string(),
        "--disable-extensions".to_string(),
        "--disable-background-networking".to_string(),
        "--disable-sync".to_string(),
        "--force-device-scale-factor=1".to_string(),
        "--hide-scrollbars".to_string(),
        "--blink-settings=imagesEnabled=false".to_string(),
        "--disable-features=OverlayScrollbar".to_string(),
        "--allow-file-access-from-files".to_string(),
        "--force-color-profile=sRGB".to_string(),
        "--window-size=800,600".to_string(),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--font-render-hinting=none".to_string(),
        "--disable-font-subpixel-positioning".to_string(),
    ]
}

/// Start Chrome and connect via CDP.
pub async fn start_and_connect() -> Result<BrowserWithHandler> {
    if is_chrome_running() {
        kill_existing_chrome().await;
    }

    let chrome_bin = find_chrome().await?;

    let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target");
    let user_data_dir = target_dir.join("chrome_test_data");
    if user_data_dir.exists() {
        let _ = std::fs::remove_dir_all(&user_data_dir);
    }
    std::fs::create_dir_all(&user_data_dir)?;

    let args = chrome_args(&user_data_dir);
    let mut process = Command::new(&chrome_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| anyhow!("Failed to start Chrome: {err}"))?;

    let max_wait = Duration::from_secs(10);
    let start = Instant::now();

    while start.elapsed() < max_wait {
        if is_chrome_running() {
            break;
        }
        if let Ok(Some(status)) = process.try_wait() {
            let mut stderr_str = String::new();
            if let Some(mut stderr) = process.stderr.take() {
                let _ = stderr.read_to_string(&mut stderr_str);
            }
            return Err(anyhow!("Chrome exited with {status}: {stderr_str}"));
        }
        sleep(Duration::from_millis(100)).await;
    }

    if !is_chrome_running() {
        let _ = process.kill();
        return Err(anyhow!("Chrome failed to start within {max_wait:?}"));
    }

    let ws_url = format!("http://localhost:{CHROME_PORT}");
    let (browser, mut handler) = Browser::connect(&ws_url)
        .await
        .map_err(|err| anyhow!("Failed to connect to Chrome: {err}"))?;

    let handler_task = spawn(async move { while let Some(_event) = handler.next().await {} });

    Ok(BrowserWithHandler {
        browser,
        _handler_task: handler_task,
        chrome_process: Some(process),
        user_data_dir: Some(user_data_dir),
    })
}

/// JavaScript for layout extraction from Chrome.
const EXTRACTION_JS: &str = r#"(function() {
function shouldSkip(el) {
    if (!el || !el.tagName) return false;
    var tag = String(el.tagName).toLowerCase();
    if (tag === 'style' && el.getAttribute('data-valor-test-reset') === '1') return true;
    try {
        var cs = window.getComputedStyle(el);
        if (cs && String(cs.display||'').toLowerCase() === 'none') return true;
    } catch (e) {}
    return false;
}
function pickStyle(el, cs) {
    var d = String(cs.display || '').toLowerCase();
    function pickEdges(prefix) {
        return {
            top: cs[prefix + 'Top'] || '',
            right: cs[prefix + 'Right'] || '',
            bottom: cs[prefix + 'Bottom'] || '',
            left: cs[prefix + 'Left'] || ''
        };
    }
    return {
        display: d,
        boxSizing: (cs.boxSizing || '').toLowerCase(),
        flexBasis: cs.flexBasis || '',
        flexGrow: Number(cs.flexGrow || 0),
        flexShrink: Number(cs.flexShrink || 0),
        margin: pickEdges('margin'),
        padding: pickEdges('padding'),
        borderWidth: {
            top: cs.borderTopWidth || '',
            right: cs.borderRightWidth || '',
            bottom: cs.borderBottomWidth || '',
            left: cs.borderLeftWidth || ''
        },
        alignItems: (cs.alignItems || '').toLowerCase(),
        overflow: (cs.overflow || '').toLowerCase(),
        position: (cs.position || '').toLowerCase(),
        fontSize: cs.fontSize || '',
        fontWeight: cs.fontWeight || '',
        fontFamily: cs.fontFamily || '',
        color: cs.color || '',
        lineHeight: cs.lineHeight || '',
        zIndex: cs.zIndex || 'auto',
        opacity: cs.opacity || '1'
    };
}
function serText(textNode, parentEl) {
    var text = textNode.textContent || '';
    if (!text || /^\s*$/.test(text)) return null;
    var range = document.createRange();
    range.selectNodeContents(textNode);
    var r = range.getBoundingClientRect();
    var cs = window.getComputedStyle(parentEl);
    return {
        type: 'text',
        text: text,
        rect: { x: r.x, y: r.y, width: r.width, height: r.height },
        style: {
            fontSize: cs.fontSize || '',
            fontWeight: cs.fontWeight || '',
            color: cs.color || '',
            lineHeight: cs.lineHeight || ''
        }
    };
}
function serNode(node, parentEl) {
    if (node.nodeType === 3) return serText(node, parentEl || node.parentElement);
    if (node.nodeType === 1) return serElement(node);
    return null;
}
function serElement(el) {
    var r = el.getBoundingClientRect();
    var cs = window.getComputedStyle(el);
    var attrs = {};
    if (el.hasAttribute('type')) attrs.type = el.getAttribute('type');
    if (el.hasAttribute('checked')) attrs.checked = 'true';
    var tag = String(el.tagName||'').toLowerCase();
    var isFormControl = tag === 'input' || tag === 'textarea' || tag === 'select' || tag === 'button';
    var children = [];
    if (!isFormControl) {
        for (var i = 0; i < el.childNodes.length; i++) {
            var child = el.childNodes[i];
            if (child.nodeType === 1 && shouldSkip(child)) continue;
            var serialized = serNode(child, el);
            if (serialized) children.push(serialized);
        }
    }
    return {
        type: 'element',
        tag: tag,
        id: String(el.id||''),
        attrs: attrs,
        rect: { x: r.x, y: r.y, width: r.width, height: r.height },
        style: pickStyle(el, cs),
        children: children
    };
}
var root = document.body || document.documentElement;
var layout = serElement(root);
var asserts = Array.isArray(window._valorResults) ? window._valorResults : [];
return JSON.stringify({ layout: layout, asserts: asserts });
})()"#;

/// Extract layout JSON from a Chrome page for a given fixture.
pub async fn extract_layout(page: &Page, fixture_path: &Path) -> Result<JsonValue> {
    let url = common::to_file_url(fixture_path).map_err(|err| anyhow!("{err}"))?;
    page.goto(&url).await?;

    // Inject CSS reset
    page.evaluate(common::css_reset_injection_script()).await?;

    // Set viewport
    let viewport = SetDeviceMetricsOverrideParams::builder()
        .width(i64::from(common::VIEWPORT_WIDTH))
        .height(i64::from(common::VIEWPORT_HEIGHT))
        .device_scale_factor(1.0)
        .mobile(false)
        .build()
        .map_err(|err| anyhow!("viewport params: {err}"))?;
    page.execute(viewport).await?;

    // Extract layout
    let result = page.evaluate(EXTRACTION_JS).await?;
    let value = result
        .value()
        .ok_or_else(|| anyhow!("No value from Chrome evaluate"))?;
    let json_string = value
        .as_str()
        .ok_or_else(|| anyhow!("Chrome returned non-string"))?;
    let parsed: JsonValue = from_str(json_string)?;
    Ok(parsed)
}

/// Get Chrome layout JSON, using cache if available.
pub async fn get_layout_cached(page: &Page, fixture_path: &Path) -> Result<JsonValue> {
    if let Some(cached) = cache::read_cached(fixture_path) {
        return Ok(cached);
    }

    let json = extract_layout(page, fixture_path).await?;
    cache::write_cache(fixture_path, &json);
    Ok(json)
}
