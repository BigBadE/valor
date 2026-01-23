//! Compare rewrite implementation output against Chrome's computed styles.

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures_util::StreamExt;
use rewrite_core::DependencyContext;
use rewrite_css::storage::{CssValueQuery, ResolvedMarginQuery};
use rewrite_html::{ChildrenQuery, TagNameQuery};
use rewrite_page::Page;
use std::fs;
use std::path::Path;

/// Helper to find an element by tag name in the DOM tree
fn find_element_by_tag(
    db: &rewrite_core::Database,
    node: rewrite_core::NodeId,
    tag: &str,
    ctx: &mut DependencyContext,
) -> Option<rewrite_core::NodeId> {
    if let Some(node_tag) = db.query::<TagNameQuery>(node, ctx) {
        if node_tag == tag {
            return Some(node);
        }
    }

    let children = db.query::<ChildrenQuery>(node, ctx);
    for &child in &children {
        if let Some(found) = find_element_by_tag(db, child, tag, ctx) {
            return Some(found);
        }
    }
    None
}

/// Get multiple computed CSS properties from Chrome at once
async fn get_chrome_computed_properties(
    page: &chromiumoxide::Page,
    selector: &str,
    properties: &[&str],
) -> Result<std::collections::HashMap<String, String>, Box<dyn std::error::Error>> {
    let properties_json = serde_json::to_string(properties)?;
    let script = format!(
        r#"
        (() => {{
            const el = document.querySelector('{}');
            if (!el) throw new Error('Element not found');
            const style = window.getComputedStyle(el);
            const properties = {};
            const result = {{}};
            for (const prop of properties) {{
                result[prop] = style.getPropertyValue(prop);
            }}
            return result;
        }})();
        "#,
        selector, properties_json
    );

    let result = page.evaluate(script).await?;
    let value = result.value().ok_or("No value returned")?;

    let map: std::collections::HashMap<String, String> = serde_json::from_value(value.clone())
        .map_err(|e| format!("Failed to parse result: {}", e))?;

    Ok(map)
}

/// Parse Chrome's computed value (e.g., "10px" -> 10.0)
fn parse_chrome_px_value(value: &str) -> Option<f64> {
    value.trim().strip_suffix("px")?.parse().ok()
}

/// Parse Chrome's rgb() color format (e.g., "rgb(255, 0, 0)" -> (255, 0, 0))
fn parse_chrome_rgb_value(value: &str) -> Option<(u8, u8, u8)> {
    let value = value.trim();
    if !value.starts_with("rgb(") || !value.ends_with(')') {
        return None;
    }

    let inner = &value[4..value.len() - 1];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }

    let r = parts[0].trim().parse().ok()?;
    let g = parts[1].trim().parse().ok()?;
    let b = parts[2].trim().parse().ok()?;

    Some((r, g, b))
}

/// Parse Valor's color value to RGB
fn parse_valor_color(value: &rewrite_css::CssValue) -> Option<(u8, u8, u8)> {
    match value {
        rewrite_css::CssValue::Color(color) => Some((color.r, color.g, color.b)),
        _ => None,
    }
}

/// Compare a single CSS property between Chrome and Valor
fn compare_property(
    chrome_value: &str,
    valor_node: rewrite_core::NodeId,
    property: &str,
    db: &rewrite_core::Database,
    ctx: &mut DependencyContext,
) -> Result<bool, String> {
    use rewrite_css::storage::InheritedCssPropertyQuery;

    // Handle auto keyword
    if chrome_value.trim() == "auto" {
        let valor_value = db.query::<CssValueQuery>((valor_node, property.to_string()), ctx);
        if valor_value.is_auto() {
            return Ok(true);
        } else {
            return Err(format!(
                "Property '{}': Chrome='auto', Valor={} subpixels",
                property,
                valor_value.subpixels_or_zero()
            ));
        }
    }

    // Try parsing as rgb() color
    if let Some(chrome_rgb) = parse_chrome_rgb_value(chrome_value) {
        // Get raw CSS value (not resolved to subpixels) for color properties
        let valor_css_value =
            db.query::<InheritedCssPropertyQuery>((valor_node, property.to_string()), ctx);

        if let Some(valor_rgb) = parse_valor_color(&valor_css_value) {
            if chrome_rgb == valor_rgb {
                return Ok(true);
            } else {
                return Err(format!(
                    "Property '{}': Chrome=rgb({}, {}, {}), Valor=rgb({}, {}, {})",
                    property,
                    chrome_rgb.0,
                    chrome_rgb.1,
                    chrome_rgb.2,
                    valor_rgb.0,
                    valor_rgb.1,
                    valor_rgb.2
                ));
            }
        } else {
            return Err(format!(
                "Property '{}': Chrome=rgb({}, {}, {}), Valor has no color value",
                property, chrome_rgb.0, chrome_rgb.1, chrome_rgb.2
            ));
        }
    }

    // Parse as numeric values (px)
    let chrome_px = parse_chrome_px_value(chrome_value)
        .ok_or_else(|| format!("Failed to parse Chrome value: {}", chrome_value))?;

    // For margin properties, use ResolvedMarginQuery which handles auto margins
    let valor_px = if property.starts_with("margin-") {
        let resolved_margin =
            db.query::<ResolvedMarginQuery>((valor_node, property.to_string()), ctx);
        (resolved_margin as f64) / 64.0
    } else {
        let valor_value = db.query::<CssValueQuery>((valor_node, property.to_string()), ctx);
        (valor_value.subpixels_or_zero() as f64) / 64.0
    };

    // Compare exact values
    if chrome_px == valor_px {
        Ok(true)
    } else {
        Err(format!(
            "Property '{}': Chrome={:.2}px, Valor={:.2}px",
            property, chrome_px, valor_px
        ))
    }
}

/// Generic fixture comparison test
async fn compare_fixture_against_chrome(
    fixture_name: &str,
    selector: &str,
    properties: Vec<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Start Chrome
    let (mut browser, mut handler) = Browser::launch(BrowserConfig::builder().build()?).await?;

    let handle = tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    // Create a page
    let page = browser.new_page("about:blank").await?;

    // Load the fixture HTML
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture_name);
    let html = fs::read_to_string(&fixture_path)?;

    // Navigate Chrome to the HTML
    let data_url = format!(
        "data:text/html;charset=utf-8,{}",
        urlencoding::encode(&html)
    );
    page.goto(&data_url).await?;
    page.wait_for_navigation().await?;

    // Get Chrome's viewport size
    let viewport_script = "({ width: window.innerWidth, height: window.innerHeight })";
    let viewport_result = page.evaluate(viewport_script).await?;
    let viewport_value = viewport_result.value().ok_or("No viewport value")?;
    let viewport_width = viewport_value["width"].as_f64().unwrap_or(1920.0) as f32;
    let viewport_height = viewport_value["height"].as_f64().unwrap_or(1080.0) as f32;

    // Parse with Valor using the same viewport
    let valor_page = Page::from_html_with_viewport(&html, Some((viewport_width, viewport_height)));
    let db = valor_page.database();
    let mut ctx = DependencyContext::new();

    // Find the target element in Valor (extract tag from selector)
    let tag = selector.trim_start_matches("div");
    let tag = if tag.is_empty() { "div" } else { tag };
    let target = find_element_by_tag(db, valor_page.root(), tag, &mut ctx)
        .ok_or_else(|| format!("Failed to find {} in Valor", tag))?;

    // Fetch all Chrome computed properties at once
    let chrome_properties = get_chrome_computed_properties(&page, selector, &properties).await?;

    // Compare properties
    let mut passed = 0;
    let mut failed = 0;
    let mut errors = Vec::new();

    for property in properties {
        let chrome_value = chrome_properties
            .get(property)
            .ok_or_else(|| format!("Chrome didn't return value for {}", property))?;

        match compare_property(chrome_value, target, property, db, &mut ctx) {
            Ok(true) => {
                println!("✓ {}: MATCH", property);
                passed += 1;
            }
            Ok(false) => {
                println!("✗ {}: MISMATCH", property);
                errors.push(format!("{}: MISMATCH", property));
                failed += 1;
            }
            Err(e) => {
                println!("✗ {}: {}", property, e);
                errors.push(format!("{}: {}", property, e));
                failed += 1;
            }
        }
    }

    // Cleanup
    browser.close().await?;
    handle.await?;

    println!("\n=== Summary ===");
    println!("Passed: {}/{}", passed, passed + failed);
    println!("Failed: {}/{}", failed, passed + failed);

    if !errors.is_empty() {
        return Err(format!("Comparison failed:\n{}", errors.join("\n")).into());
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn compare_simple_box_against_chrome() -> Result<(), Box<dyn std::error::Error>> {
    compare_fixture_against_chrome(
        "simple_box.html",
        "div",
        vec![
            "width",
            "height",
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
            "margin-top",
            "margin-right",
            "margin-bottom",
            "margin-left",
        ],
    )
    .await
}

#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn compare_nested_inheritance_against_chrome() -> Result<(), Box<dyn std::error::Error>> {
    compare_fixture_against_chrome("nested_inheritance.html", "div", vec!["font-size", "color"])
        .await
}

#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn compare_shorthand_expansion_against_chrome() -> Result<(), Box<dyn std::error::Error>> {
    compare_fixture_against_chrome(
        "shorthand_expansion.html",
        "div",
        vec![
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
            "margin-top",
            "margin-right",
            "margin-bottom",
            "margin-left",
        ],
    )
    .await
}

#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn compare_auto_margins_against_chrome() -> Result<(), Box<dyn std::error::Error>> {
    compare_fixture_against_chrome(
        "auto_margins.html",
        "div",
        vec!["margin-left", "margin-right"],
    )
    .await
}

#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn compare_rem_units_against_chrome() -> Result<(), Box<dyn std::error::Error>> {
    // Test the <p> element which has rem units in padding and font-size
    compare_fixture_against_chrome("rem_units.html", "p", vec!["padding-top", "font-size"]).await
}

#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn compare_em_units_against_chrome() -> Result<(), Box<dyn std::error::Error>> {
    compare_fixture_against_chrome("em_units.html", "div", vec!["font-size"]).await
}

#[tokio::test]
#[ignore] // Requires Chrome to be installed
async fn compare_color_inheritance_against_chrome() -> Result<(), Box<dyn std::error::Error>> {
    compare_fixture_against_chrome("color_inheritance.html", "div", vec!["color"]).await
}
