//! Chromium layout extraction JavaScript and execution logic.

use anyhow::{Result, anyhow};
use chromiumoxide::page::Page;
use serde_json::{Value as JsonValue, from_str};
use std::path::Path;

use super::super::chrome::navigate_and_prepare_page;

/// JavaScript helpers for Chromium layout extraction.
const CHROMIUM_SCRIPT_HELPERS: &str = "function shouldSkip(el) {
    if (!el || !el.tagName) return false;
    var tag = String(el.tagName).toLowerCase();
    if (tag === 'style' && el.getAttribute('data-valor-test-reset') === '1') return true;
    try {
        var cs = window.getComputedStyle(el);
        if (cs && String(cs.display||'').toLowerCase() === 'none') return true;
    } catch (e) { /* ignore */ }
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
            left: cs.borderLeftWidth || '',
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
}";

/// JavaScript serializers for layout extraction.
const CHROMIUM_SCRIPT_SERIALIZERS: &str = "function serText(textNode, parentEl) {
    var text = textNode.textContent || '';
    if (!text || /^\\s*$/.test(text)) return null;
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
    if (node.nodeType === 3) {
        return serText(node, parentEl || node.parentElement);
    }
    if (node.nodeType === 1) {
        return serElement(node);
    }
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
}";

/// Main Chromium layout extraction script.
const CHROMIUM_SCRIPT_MAIN: &str = "if (!window._valorResults) { window._valorResults = []; }
if (typeof window._valorAssert !== 'function') {
    window._valorAssert = function(name, cond, details) {
        window._valorResults.push({ name: String(name||''), ok: !!cond, details: String(details||'') });
    };
}
if (typeof window._valorRun === 'function') {
    try { window._valorRun(); } catch (e) {
        window._valorResults.push({ name: '_valorRun', ok: false, details: String(e && e.stack || e) });
    }
}
var root = document.body || document.documentElement;
var layout = serElement(root);
var asserts = Array.isArray(window._valorResults) ? window._valorResults : [];
return JSON.stringify({ layout: layout, asserts: asserts });";

/// Build complete Chromium layout extraction script.
fn chromium_layout_extraction_script() -> String {
    format!(
        "(function() {{ {CHROMIUM_SCRIPT_HELPERS} {CHROMIUM_SCRIPT_SERIALIZERS} {CHROMIUM_SCRIPT_MAIN} }})()"
    )
}

/// Extracts layout JSON from Chromium by evaluating JavaScript in a page.
///
/// # Errors
///
/// Returns an error if navigation, script evaluation, or JSON parsing fails.
pub async fn chromium_layout_json_in_page(page: &Page, path: &Path) -> Result<JsonValue> {
    navigate_and_prepare_page(page, path).await?;
    let script = chromium_layout_extraction_script();
    let result = page.evaluate(script).await?;
    let value = result
        .value()
        .ok_or_else(|| anyhow!("No value returned from Chromium evaluate"))?;
    let json_string = value
        .as_str()
        .ok_or_else(|| anyhow!("Chromium returned non-string JSON for layout"))?;
    let parsed: JsonValue = from_str(json_string)?;
    Ok(parsed)
}
