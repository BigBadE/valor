//! Chromium comparison tests - runs all fixtures from fixtures/ directory.

use anyhow::Result;
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures_util::StreamExt;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

/// Get all HTML fixture files from the fixtures directory.
fn get_fixture_files() -> Result<Vec<PathBuf>> {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let mut files = Vec::new();

    if fixtures_dir.exists() {
        for entry in fs::read_dir(fixtures_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("html") {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

/// Get Chromium's layout tree.
async fn get_chromium_layout(html: &str) -> Result<serde_json::Value> {
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .window_size(1024, 768)
            .build()
            .map_err(|e| anyhow::anyhow!("Browser config error: {}", e))?,
    )
    .await?;

    let handle = tokio::task::spawn(async move { while handler.next().await.is_some() {} });

    let page = browser.new_page("about:blank").await?;
    page.set_content(html).await?;

    // Wait for layout
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get layout tree from Chromium
    let layout = page
        .evaluate(
            r#"(function() {
            function serializeNode(node) {
                if (node.nodeType === Node.TEXT_NODE) {
                    if (!node.textContent.trim()) return null;
                    return {
                        type: "text",
                        text: node.textContent
                    };
                }
                if (node.nodeType === Node.ELEMENT_NODE) {
                    const rect = node.getBoundingClientRect();
                    const style = window.getComputedStyle(node);
                    const children = Array.from(node.childNodes)
                        .map(serializeNode)
                        .filter(c => c !== null);
                    return {
                        type: "element",
                        tagName: node.tagName.toLowerCase(),
                        rect: {
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height
                        },
                        style: {
                            display: style.display,
                            position: style.position,
                            width: style.width,
                            height: style.height
                        },
                        children
                    };
                }
                if (node.nodeType === Node.DOCUMENT_NODE) {
                    const children = Array.from(node.childNodes)
                        .map(serializeNode)
                        .filter(c => c !== null);
                    return {
                        type: "document",
                        children
                    };
                }
                return null;
            }
            return serializeNode(document);
        })()"#,
        )
        .await?;

    browser.close().await?;
    handle.abort();

    Ok(layout.into_value()?)
}

/// Compare two layout trees and return differences.
fn compare_layouts(
    valor: &serde_json::Value,
    chromium: &serde_json::Value,
    path: &str,
) -> Vec<String> {
    let mut diffs = Vec::new();

    // Compare node types
    let valor_type = valor.get("type").and_then(|v| v.as_str());
    let chromium_type = chromium.get("type").and_then(|v| v.as_str());

    if valor_type != chromium_type {
        diffs.push(format!(
            "{}: type mismatch - valor: {:?}, chromium: {:?}",
            path, valor_type, chromium_type
        ));
        return diffs;
    }

    match valor_type {
        Some("element") => {
            // Compare tag names
            let valor_tag = valor.get("tagName").and_then(|v| v.as_str());
            let chromium_tag = chromium.get("tagName").and_then(|v| v.as_str());
            if valor_tag != chromium_tag {
                diffs.push(format!(
                    "{}: tag mismatch - valor: {:?}, chromium: {:?}",
                    path, valor_tag, chromium_tag
                ));
            }

            // Compare rects (with tolerance for floating point)
            if let (Some(valor_rect), Some(chromium_rect)) =
                (valor.get("rect"), chromium.get("rect"))
            {
                for prop in &["x", "y", "width", "height"] {
                    let valor_val = valor_rect.get(prop).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let chromium_val = chromium_rect
                        .get(prop)
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    if (valor_val - chromium_val).abs() > 1.0 {
                        diffs.push(format!(
                            "{}: rect.{} mismatch - valor: {:.2}, chromium: {:.2}",
                            path, prop, valor_val, chromium_val
                        ));
                    }
                }
            }

            // Compare children
            if let (Some(valor_children), Some(chromium_children)) = (
                valor.get("children").and_then(|v| v.as_array()),
                chromium.get("children").and_then(|v| v.as_array()),
            ) {
                if valor_children.len() != chromium_children.len() {
                    diffs.push(format!(
                        "{}: children count mismatch - valor: {}, chromium: {}",
                        path,
                        valor_children.len(),
                        chromium_children.len()
                    ));
                } else {
                    for (i, (v_child, c_child)) in valor_children
                        .iter()
                        .zip(chromium_children.iter())
                        .enumerate()
                    {
                        diffs.extend(compare_layouts(
                            v_child,
                            c_child,
                            &format!("{}[{}]", path, i),
                        ));
                    }
                }
            }
        }
        Some("text") => {
            let valor_text = valor.get("text").and_then(|v| v.as_str());
            let chromium_text = chromium.get("text").and_then(|v| v.as_str());
            if valor_text != chromium_text {
                diffs.push(format!(
                    "{}: text mismatch - valor: {:?}, chromium: {:?}",
                    path, valor_text, chromium_text
                ));
            }
        }
        Some("document") => {
            // Compare document children
            if let (Some(valor_children), Some(chromium_children)) = (
                valor.get("children").and_then(|v| v.as_array()),
                chromium.get("children").and_then(|v| v.as_array()),
            ) {
                if valor_children.len() != chromium_children.len() {
                    diffs.push(format!(
                        "{}: document children count mismatch - valor: {}, chromium: {}",
                        path,
                        valor_children.len(),
                        chromium_children.len()
                    ));
                } else {
                    for (i, (v_child, c_child)) in valor_children
                        .iter()
                        .zip(chromium_children.iter())
                        .enumerate()
                    {
                        diffs.extend(compare_layouts(
                            v_child,
                            c_child,
                            &format!("{}[{}]", path, i),
                        ));
                    }
                }
            }
        }
        _ => {}
    }

    diffs
}

/// Serialize Valor's layout tree.
fn serialize_valor_layout(_page: &rewrite_page::Page) -> Result<serde_json::Value> {
    // Layout computation not yet implemented after Database refactor
    anyhow::bail!("Layout serialization not implemented - need to add layout computation system")
}

/// Run all fixture tests against Chromium.
#[tokio::test]
async fn test_chromium_fixtures() -> Result<()> {
    let files = get_fixture_files()?;

    println!("Found {} fixture files", files.len());

    let mut passed = 0;
    let mut failed = 0;

    for file in files {
        let name = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        println!("\nTesting: {}", name);

        let html = fs::read_to_string(&file)?;

        // Create page and navigate
        let mut page = rewrite_page::Page::new();
        page.navigate(html.clone());

        // Wait for async parsing
        thread::sleep(Duration::from_millis(100));

        // Get Chromium's layout
        let chromium_layout = get_chromium_layout(&html).await?;

        // Get Valor's layout
        let valor_layout = serialize_valor_layout(&page)?;

        // Compare layouts
        let diffs = compare_layouts(&valor_layout, &chromium_layout, "root");

        if diffs.is_empty() {
            println!("  ✓ {} - PASS", name);
            passed += 1;
        } else {
            println!("  ✗ {} - FAIL ({} differences)", name, diffs.len());
            for diff in diffs.iter().take(5) {
                println!("    - {}", diff);
            }
            if diffs.len() > 5 {
                println!("    ... and {} more", diffs.len() - 5);
            }
            failed += 1;
        }
    }

    println!("\n========================================");
    println!("Results: {} passed, {} failed", passed, failed);
    println!("========================================");

    if failed > 0 {
        anyhow::bail!("{} fixtures failed", failed);
    }

    Ok(())
}
