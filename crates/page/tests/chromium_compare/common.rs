use std::path::Path;

pub const VIEWPORT_WIDTH: u32 = 800;
pub const VIEWPORT_HEIGHT: u32 = 600;

/// CSS reset applied to both Chrome and Valor for consistent baselines.
pub const CSS_RESET: &str = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;font-family:monospace !important;font-size:13px !important;}html,body{margin:0 !important;padding:0 !important;overflow:hidden;font-family:monospace !important;font-size:13px !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;font-family:monospace !important;}ul,ol{margin:0;padding:0;list-style:none;}";

/// Prepend CSS reset as a `<style>` tag into HTML for Valor.
pub fn prepend_css_reset(html: &str) -> String {
    let reset_tag = format!("<style data-valor-test-reset=\"1\">{CSS_RESET}</style>");

    // Insert after <head> if present
    if let Some(pos) = html.find("<head>") {
        let insert_pos = pos + "<head>".len();
        return format!("{}{reset_tag}{}", &html[..insert_pos], &html[insert_pos..]);
    }
    if let Some(pos) = html.find("<head ") {
        if let Some(end) = html[pos..].find('>') {
            let insert_pos = pos + end + 1;
            return format!("{}{reset_tag}{}", &html[..insert_pos], &html[insert_pos..]);
        }
    }

    // Insert before <body> if no <head>
    if let Some(pos) = html.find("<body") {
        return format!("{}{reset_tag}{}", &html[..pos], &html[pos..]);
    }

    // Fallback: prepend
    format!("{reset_tag}{html}")
}

/// JavaScript to inject CSS reset into Chrome page.
pub fn css_reset_injection_script() -> String {
    format!(
        "(function(){{
        try {{
            var css = '{CSS_RESET}';
            var existing = document.querySelector('style[data-valor-test-reset=\"1\"]');
            if (existing) {{ return true; }}
            var style = document.createElement('style');
            style.setAttribute('data-valor-test-reset','1');
            style.type = 'text/css';
            style.appendChild(document.createTextNode(css));
            var head = document.head || document.getElementsByTagName('head')[0] || document.documentElement;
            head.appendChild(style);
            var de = document.documentElement; if (de && de.style){{ de.style.margin='0'; de.style.padding='0'; }}
            var b = document.body; if (b && b.style){{ b.style.margin='0'; b.style.padding='0'; }}
            void (document.body && document.body.offsetWidth);
            return true;
        }} catch (e) {{
            return false;
        }}
    }})()"
    )
}

/// Convert a file path to a file:// URL.
pub fn to_file_url(path: &Path) -> Result<String, String> {
    let canonical = path
        .canonicalize()
        .map_err(|err| format!("Failed to canonicalize path: {err}"))?;
    Ok(format!("file://{}", canonical.display()))
}
