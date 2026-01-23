//! Fixture-based tests that compare rewrite implementation against expected layout output.

use rewrite_core::DependencyContext;
use rewrite_css::storage::CssValueQuery;
use rewrite_html::{ChildrenQuery, NodeData, TagNameQuery};
use rewrite_layout::{BlockOffsetQuery, BlockSizeQuery, InlineOffsetQuery, InlineSizeQuery};
use rewrite_page::Page;
use serde_json::{Value as JsonValue, json};
use std::fs;
use std::path::Path;

/// Generate layout JSON for a page (similar to Chromium's output format).
fn generate_layout_json(page: &Page) -> JsonValue {
    let db = page.database();
    let mut ctx = DependencyContext::new();

    fn serialize_node(
        db: &rewrite_core::Database,
        node: rewrite_core::NodeId,
        ctx: &mut DependencyContext,
    ) -> JsonValue {
        use rewrite_core::NodeDataExt;

        // Get node data
        let node_data = db.get_node_data::<NodeData>(node);

        let mut obj = json!({});

        match node_data {
            Some(NodeData::Document) => {
                obj["type"] = json!("document");
            }
            Some(NodeData::Element(elem)) => {
                obj["type"] = json!("element");
                obj["tag"] = json!(elem.tag_name);

                // Get computed styles
                let width = db.query::<CssValueQuery>((node, "width".to_string()), ctx);
                let height = db.query::<CssValueQuery>((node, "height".to_string()), ctx);
                let padding_top = db.query::<CssValueQuery>((node, "padding-top".to_string()), ctx);
                let padding_right =
                    db.query::<CssValueQuery>((node, "padding-right".to_string()), ctx);
                let padding_bottom =
                    db.query::<CssValueQuery>((node, "padding-bottom".to_string()), ctx);
                let padding_left =
                    db.query::<CssValueQuery>((node, "padding-left".to_string()), ctx);
                let margin_top = db.query::<CssValueQuery>((node, "margin-top".to_string()), ctx);
                let margin_right =
                    db.query::<CssValueQuery>((node, "margin-right".to_string()), ctx);
                let margin_bottom =
                    db.query::<CssValueQuery>((node, "margin-bottom".to_string()), ctx);
                let margin_left = db.query::<CssValueQuery>((node, "margin-left".to_string()), ctx);
                let border_top_width =
                    db.query::<CssValueQuery>((node, "border-top-width".to_string()), ctx);
                let border_right_width =
                    db.query::<CssValueQuery>((node, "border-right-width".to_string()), ctx);
                let border_bottom_width =
                    db.query::<CssValueQuery>((node, "border-bottom-width".to_string()), ctx);
                let border_left_width =
                    db.query::<CssValueQuery>((node, "border-left-width".to_string()), ctx);

                // Query actual layout dimensions and positions
                let layout_width = db.query::<InlineSizeQuery>(node, ctx);
                let layout_height = db.query::<BlockSizeQuery>(node, ctx);
                let layout_x = db.query::<InlineOffsetQuery>(node, ctx);
                let layout_y = db.query::<BlockOffsetQuery>(node, ctx);

                // Convert subpixels to pixels (divide by 64)
                let to_px = |subpixels: i32| (subpixels as f64) / 64.0;

                obj["computed"] = json!({
                    "width": if width.is_auto() { json!("auto") } else { json!(to_px(width.subpixels_or_zero())) },
                    "height": if height.is_auto() { json!("auto") } else { json!(to_px(height.subpixels_or_zero())) },
                    "padding": {
                        "top": to_px(padding_top.subpixels_or_zero()),
                        "right": to_px(padding_right.subpixels_or_zero()),
                        "bottom": to_px(padding_bottom.subpixels_or_zero()),
                        "left": to_px(padding_left.subpixels_or_zero()),
                    },
                    "margin": {
                        "top": if margin_top.is_auto() { json!("auto") } else { json!(to_px(margin_top.subpixels_or_zero())) },
                        "right": if margin_right.is_auto() { json!("auto") } else { json!(to_px(margin_right.subpixels_or_zero())) },
                        "bottom": if margin_bottom.is_auto() { json!("auto") } else { json!(to_px(margin_bottom.subpixels_or_zero())) },
                        "left": if margin_left.is_auto() { json!("auto") } else { json!(to_px(margin_left.subpixels_or_zero())) },
                    },
                    "border": {
                        "top": to_px(border_top_width.subpixels_or_zero()),
                        "right": to_px(border_right_width.subpixels_or_zero()),
                        "bottom": to_px(border_bottom_width.subpixels_or_zero()),
                        "left": to_px(border_left_width.subpixels_or_zero()),
                    },
                });

                // Add actual layout dimensions
                obj["layout"] = json!({
                    "width": to_px(layout_width),
                    "height": to_px(layout_height),
                    "x": to_px(layout_x),
                    "y": to_px(layout_y),
                });
            }
            Some(NodeData::Text(text)) => {
                obj["type"] = json!("text");
                obj["text"] = json!(text);
            }
            Some(NodeData::Comment(_)) => {
                obj["type"] = json!("comment");
            }
            None => {
                obj["type"] = json!("unknown");
            }
        }

        // Serialize children
        let children = db.query::<ChildrenQuery>(node, ctx);
        if !children.is_empty() {
            obj["children"] = json!(
                children
                    .iter()
                    .map(|&child| serialize_node(db, child, ctx))
                    .collect::<Vec<_>>()
            );
        }

        obj
    }

    serialize_node(db, page.root(), &mut ctx)
}

/// Run a fixture test and print the layout JSON.
fn run_fixture(html_path: &Path) {
    let html = fs::read_to_string(html_path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", html_path.display()));

    let page = Page::from_html(&html);
    let layout_json = generate_layout_json(&page);

    println!("\n=== Fixture: {} ===", html_path.display());
    println!("{}", serde_json::to_string_pretty(&layout_json).unwrap());
}

#[test]
fn test_simple_box_fixture() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple_box.html");

    if fixture_path.exists() {
        run_fixture(&fixture_path);

        // Validate specific properties
        let html = fs::read_to_string(&fixture_path).unwrap();
        let page = Page::from_html(&html);
        let db = page.database();
        let mut ctx = DependencyContext::new();

        // Find the div
        fn find_div(
            db: &rewrite_core::Database,
            node: rewrite_core::NodeId,
            ctx: &mut DependencyContext,
        ) -> Option<rewrite_core::NodeId> {
            if let Some(tag) = db.query::<TagNameQuery>(node, ctx) {
                if tag == "div" {
                    return Some(node);
                }
            }
            let children = db.query::<ChildrenQuery>(node, ctx);
            for &child in &children {
                if let Some(found) = find_div(db, child, ctx) {
                    return Some(found);
                }
            }
            None
        }

        let div = find_div(db, page.root(), &mut ctx).expect("Should find div");

        // Verify computed CSS values
        let width = db.query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
        assert_eq!(width.subpixels_or_zero(), 200 * 64, "Width should be 200px");

        let padding_top = db.query::<CssValueQuery>((div, "padding-top".to_string()), &mut ctx);
        assert_eq!(
            padding_top.subpixels_or_zero(),
            10 * 64,
            "Padding-top should be 10px"
        );

        let margin_top = db.query::<CssValueQuery>((div, "margin-top".to_string()), &mut ctx);
        assert_eq!(
            margin_top.subpixels_or_zero(),
            20 * 64,
            "Margin-top should be 20px"
        );

        // Verify actual layout dimensions
        // simple_box.html: <div style="width: 200px; height: 100px; padding: 10px; margin: 20px; border-width: 5px;">
        // Layout width = 200 (content) + 10*2 (padding) + 5*2 (border) = 230px
        // Layout height = 100 (content) + 10*2 (padding) + 5*2 (border) = 130px
        let layout_width = db.query::<InlineSizeQuery>(div, &mut ctx);
        let layout_height = db.query::<BlockSizeQuery>(div, &mut ctx);
        let layout_x = db.query::<InlineOffsetQuery>(div, &mut ctx);
        let layout_y = db.query::<BlockOffsetQuery>(div, &mut ctx);

        assert_eq!(
            layout_width,
            230 * 64,
            "Layout width should be 230px (200 content + 20 padding + 10 border)"
        );
        assert_eq!(
            layout_height,
            130 * 64,
            "Layout height should be 130px (100 content + 20 padding + 10 border)"
        );
        assert_eq!(layout_x, 20 * 64, "Layout X should be 20px (left margin)");
        assert_eq!(layout_y, 20 * 64, "Layout Y should be 20px (top margin)");
    }
}

#[test]
fn test_nested_inheritance_fixture() {
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nested_inheritance.html");

    if fixture_path.exists() {
        run_fixture(&fixture_path);

        let html = fs::read_to_string(&fixture_path).unwrap();
        let page = Page::from_html(&html);
        let db = page.database();
        let mut ctx = DependencyContext::new();

        // Helper to find elements
        fn find_element(
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
                if let Some(found) = find_element(db, child, tag, ctx) {
                    return Some(found);
                }
            }
            None
        }

        // Verify inheritance
        let div = find_element(db, page.root(), "div", &mut ctx).expect("Should find div");
        let div_font_size = db.query::<CssValueQuery>((div, "font-size".to_string()), &mut ctx);
        assert_eq!(
            div_font_size.subpixels_or_zero(),
            20 * 64,
            "Div font-size should be 20px"
        );

        let p = find_element(db, div, "p", &mut ctx).expect("Should find p");
        let p_width = db.query::<CssValueQuery>((p, "width".to_string()), &mut ctx);
        assert_eq!(
            p_width.subpixels_or_zero(),
            5 * 20 * 64,
            "P width should be 5em = 100px"
        );

        let span = find_element(db, p, "span", &mut ctx).expect("Should find span");
        let span_width = db.query::<CssValueQuery>((span, "width".to_string()), &mut ctx);
        assert_eq!(
            span_width.subpixels_or_zero(),
            2 * 20 * 64,
            "Span width should be 2em = 40px"
        );

        // Verify actual layout dimensions for nested elements
        let div_layout_width = db.query::<InlineSizeQuery>(div, &mut ctx);
        let p_layout_width = db.query::<InlineSizeQuery>(p, &mut ctx);
        let span_layout_width = db.query::<InlineSizeQuery>(span, &mut ctx);

        // Div has explicit font-size: 20px but no explicit width
        // P has width: 5em = 100px
        // Span has width: 2em = 40px
        assert_eq!(
            p_layout_width,
            100 * 64,
            "P layout width should be 100px (5em)"
        );
        assert_eq!(
            span_layout_width,
            40 * 64,
            "Span layout width should be 40px (2em)"
        );

        println!("Div layout width: {} px", div_layout_width / 64);
        println!("P layout width: {} px", p_layout_width / 64);
        println!("Span layout width: {} px", span_layout_width / 64);
    }
}

/// Test all HTML fixtures in the fixtures directory
#[test]
fn test_all_fixtures() {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    if !fixtures_dir.exists() {
        eprintln!("Fixtures directory not found: {}", fixtures_dir.display());
        return;
    }

    let mut fixture_count = 0;
    let mut passed = 0;
    let mut failed = 0;

    if let Ok(entries) = fs::read_dir(&fixtures_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("html") {
                fixture_count += 1;

                print!(
                    "Testing fixture: {} ... ",
                    path.file_name().unwrap().to_str().unwrap()
                );

                match std::panic::catch_unwind(|| {
                    run_fixture(&path);
                }) {
                    Ok(()) => {
                        println!("✓ PASS");
                        passed += 1;
                    }
                    Err(e) => {
                        println!("✗ FAIL");
                        if let Some(s) = e.downcast_ref::<String>() {
                            eprintln!("  Error: {}", s);
                        } else if let Some(s) = e.downcast_ref::<&str>() {
                            eprintln!("  Error: {}", s);
                        }
                        failed += 1;
                    }
                }
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total fixtures: {}", fixture_count);
    println!(
        "Passed: {} ({:.1}%)",
        passed,
        (passed as f64 / fixture_count as f64) * 100.0
    );
    println!(
        "Failed: {} ({:.1}%)",
        failed,
        (failed as f64 / fixture_count as f64) * 100.0
    );
}
