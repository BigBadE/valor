//! Chromium layout comparison tests for rewrite implementation.

use anyhow::Result;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page as ChromePage;
use futures_util::StreamExt;
use rewrite_core::DependencyContext;
use rewrite_html::{ChildrenQuery, NodeData};
use rewrite_layout::{BlockOffsetQuery, BlockSizeQuery, InlineOffsetQuery, InlineSizeQuery};
use rewrite_page::Page;
use serde_json::{Value as JsonValue, json};
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Serialize layout for a single node.
fn serialize_node_layout(
    page: &Page,
    node: rewrite_core::NodeId,
    ctx: &mut DependencyContext,
    body_offset: (f64, f64),
) -> JsonValue {
    use rewrite_core::NodeDataExt;

    let db = page.database();
    let node_data = db.get_node_data::<NodeData>(node);

    match node_data {
        Some(NodeData::Element(elem)) => {
            // Get layout
            let width = db.query::<InlineSizeQuery>(node, ctx);
            let height = db.query::<BlockSizeQuery>(node, ctx);
            let x = db.query::<InlineOffsetQuery>(node, ctx);
            let y = db.query::<BlockOffsetQuery>(node, ctx);

            let to_px = |sp: i32| sp as f64 / 64.0;

            // Serialize children, filtering out null values (whitespace-only text nodes)
            let children = db.query::<ChildrenQuery>(node, ctx);
            let child_nodes: Vec<JsonValue> = children
                .iter()
                .map(|&child| serialize_node_layout(page, child, ctx, body_offset))
                .filter(|v| !v.is_null())
                .collect();

            json!({
                "type": "element",
                "tagName": elem.tag_name,
                "rect": {
                    "x": to_px(x) - body_offset.0,
                    "y": to_px(y) - body_offset.1,
                    "width": to_px(width),
                    "height": to_px(height)
                },
                "children": child_nodes
            })
        }
        Some(NodeData::Text(text)) => {
            // Skip whitespace-only text nodes to match Chrome behavior
            if text.trim().is_empty() {
                return json!(null);
            }
            json!({
                "type": "text",
                "text": text
            })
        }
        Some(NodeData::Document) => {
            let children = db.query::<ChildrenQuery>(node, ctx);
            let child_nodes: Vec<JsonValue> = children
                .iter()
                .map(|&child| serialize_node_layout(page, child, ctx, body_offset))
                .filter(|v| !v.is_null())
                .collect();

            json!({
                "type": "document",
                "children": child_nodes
            })
        }
        _ => json!({"type": "other"}),
    }
}

/// Get Chromium's layout for a fixture.
async fn get_chromium_layout(chrome_page: &ChromePage, fixture_path: &Path) -> Result<JsonValue> {
    // Navigate to the fixture
    let file_url = format!("file://{}", fixture_path.canonicalize()?.display());
    chrome_page.goto(&file_url).await?;

    // Wait for page to load
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute JavaScript to get layout
    let layout_script = r#"
    (function() {
        function serializeNode(node) {
            if (node.nodeType === Node.DOCUMENT_NODE) {
                return {
                    type: 'document',
                    children: Array.from(node.childNodes)
                        .filter(n => n.nodeType !== Node.DOCUMENT_TYPE_NODE)
                        .map(serializeNode)
                        .filter(n => n !== null)
                };
            }
            if (node.nodeType === Node.ELEMENT_NODE) {
                const rect = node.getBoundingClientRect();
                return {
                    type: 'element',
                    tagName: node.tagName.toLowerCase(),
                    rect: {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height
                    },
                    children: Array.from(node.childNodes).map(serializeNode).filter(n => n !== null)
                };
            }
            if (node.nodeType === Node.TEXT_NODE) {
                // Skip whitespace-only text nodes
                if (node.textContent.trim() === '') {
                    return null;
                }
                return {
                    type: 'text',
                    text: node.textContent
                };
            }
            return { type: 'other' };
        }
        return serializeNode(document);
    })()
    "#;

    let result = chrome_page.evaluate(layout_script).await?;
    let json = result.into_value()?;

    Ok(json)
}

/// Compare two layout JSON trees exactly (no epsilon).
fn compare_layouts(chrome: &JsonValue, valor: &JsonValue, path: &str) -> Vec<String> {
    let mut errors = Vec::new();

    // Compare type
    if chrome.get("type") != valor.get("type") {
        errors.push(format!(
            "{}: type mismatch - chrome: {:?}, valor: {:?}",
            path,
            chrome.get("type"),
            valor.get("type")
        ));
        return errors;
    }

    let node_type = chrome.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if node_type == "element" {
        // Compare tagName
        if chrome.get("tagName") != valor.get("tagName") {
            errors.push(format!("{}: tagName mismatch", path));
        }

        // Compare rect exactly
        if let (Some(chrome_rect), Some(valor_rect)) = (chrome.get("rect"), valor.get("rect")) {
            for field in &["x", "y", "width", "height"] {
                if let (Some(chrome_val), Some(valor_val)) = (
                    chrome_rect.get(field).and_then(|v| v.as_f64()),
                    valor_rect.get(field).and_then(|v| v.as_f64()),
                ) {
                    if chrome_val != valor_val {
                        errors.push(format!(
                            "{}.rect.{}: chrome={:.2}, valor={:.2}",
                            path, field, chrome_val, valor_val
                        ));
                    }
                }
            }
        }

        // Compare children
        if let (Some(chrome_children), Some(valor_children)) = (
            chrome.get("children").and_then(|v| v.as_array()),
            valor.get("children").and_then(|v| v.as_array()),
        ) {
            if chrome_children.len() != valor_children.len() {
                errors.push(format!(
                    "{}: children count mismatch - chrome: {}, valor: {}",
                    path,
                    chrome_children.len(),
                    valor_children.len()
                ));
            }

            let tag = chrome
                .get("tagName")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            for (i, (chrome_child, valor_child)) in chrome_children
                .iter()
                .zip(valor_children.iter())
                .enumerate()
            {
                let child_path = format!("{}/{}[{}]", path, tag, i);
                errors.extend(compare_layouts(chrome_child, valor_child, &child_path));
            }
        }
    } else if node_type == "document" {
        // Compare children
        if let (Some(chrome_children), Some(valor_children)) = (
            chrome.get("children").and_then(|v| v.as_array()),
            valor.get("children").and_then(|v| v.as_array()),
        ) {
            for (i, (chrome_child, valor_child)) in chrome_children
                .iter()
                .zip(valor_children.iter())
                .enumerate()
            {
                let child_path = format!("{}/[{}]", path, i);
                errors.extend(compare_layouts(chrome_child, valor_child, &child_path));
            }
        }
    }

    errors
}

/// Test all fixtures in the fixtures directory.
#[tokio::test]
async fn test_all_rewrite_fixtures() -> Result<()> {
    // Collect all HTML fixtures
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    let mut fixtures = Vec::new();
    if fixtures_dir.exists() {
        collect_fixtures(&fixtures_dir, &mut fixtures);
    }

    if fixtures.is_empty() {
        eprintln!("No fixtures found in {}", fixtures_dir.display());
        return Ok(());
    }

    eprintln!("Found {} fixtures to test", fixtures.len());

    // Start Chrome
    let (browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .window_size(800, 600)
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?,
    )
    .await?;

    // Spawn handler
    let _handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                eprintln!("Chrome handler error: {}", e);
            }
        }
    });

    let chrome_page = browser.new_page("about:blank").await?;

    // Run tests
    let mut passed = 0;
    let mut failed = 0;
    let mut failures = Vec::new();

    for fixture_path in &fixtures {
        let name = fixture_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        print!("Testing {} ... ", name);

        match test_fixture(&chrome_page, fixture_path).await {
            Ok(errors) if errors.is_empty() => {
                println!("✓ PASS");
                passed += 1;
            }
            Ok(errors) => {
                println!("✗ FAIL");
                failed += 1;
                failures.push((name.to_string(), errors));
            }
            Err(e) => {
                println!("✗ ERROR: {}", e);
                failed += 1;
                failures.push((name.to_string(), vec![e.to_string()]));
            }
        }
    }

    // Print summary
    eprintln!("\n=== SUMMARY ===");
    eprintln!("Total: {}", fixtures.len());
    eprintln!(
        "Passed: {} ({:.1}%)",
        passed,
        passed as f64 / fixtures.len() as f64 * 100.0
    );
    eprintln!(
        "Failed: {} ({:.1}%)",
        failed,
        failed as f64 / fixtures.len() as f64 * 100.0
    );

    if !failures.is_empty() {
        eprintln!("\n=== FAILURES ===");
        for (name, errors) in &failures {
            eprintln!("\n{}:", name);
            for error in errors.iter().take(5) {
                eprintln!("  {}", error);
            }
            if errors.len() > 5 {
                eprintln!("  ... and {} more errors", errors.len() - 5);
            }
        }
    }

    if failed > 0 {
        anyhow::bail!("{} fixtures failed", failed);
    }

    Ok(())
}

/// Collect all HTML fixtures recursively.
fn collect_fixtures(dir: &Path, fixtures: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_fixtures(&path, fixtures);
            } else if path.extension().and_then(|s| s.to_str()) == Some("html") {
                fixtures.push(path);
            }
        }
    }
}

/// Test a single fixture.
async fn test_fixture(chrome_page: &ChromePage, fixture_path: &Path) -> Result<Vec<String>> {
    // Get Chromium layout
    let chrome_layout = get_chromium_layout(chrome_page, fixture_path).await?;

    // Get Valor layout
    let html = read_to_string(fixture_path)?;
    let page = Page::from_html_with_viewport(&html, Some((800.0, 600.0)));

    let mut ctx = DependencyContext::new();
    let valor_layout = serialize_node_layout(&page, page.root(), &mut ctx, (0.0, 0.0));

    // Compare exactly (no epsilon)
    let errors = compare_layouts(&chrome_layout, &valor_layout, "root");

    Ok(errors)
}
