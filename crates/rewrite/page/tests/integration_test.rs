//! Integration tests for the full HTML → CSS → Layout pipeline.

use rewrite_core::DependencyContext;
use rewrite_css::storage::CssValueQuery;
use rewrite_html::{ChildrenQuery, TagNameQuery};
use rewrite_page::Page;

/// Helper to find first element by tag name in the DOM tree.
fn find_element_by_tag(
    db: &rewrite_core::Database,
    node: rewrite_core::NodeId,
    tag_name: &str,
    ctx: &mut DependencyContext,
) -> Option<rewrite_core::NodeId> {
    if let Some(tag) = db.query::<TagNameQuery>(node, ctx) {
        if tag == tag_name {
            return Some(node);
        }
    }

    let children = db.query::<ChildrenQuery>(node, ctx);
    for &child in &children {
        if let Some(found) = find_element_by_tag(db, child, tag_name, ctx) {
            return Some(found);
        }
    }
    None
}

#[test]
fn test_html_parsing_with_style_attributes() {
    let html = r#"
        <html>
            <body>
                <div style="width: 200px; height: 100px; padding: 10px">
                    <p style="margin: 5px">Hello World</p>
                </div>
            </body>
        </html>
    "#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    // Find the div element
    let div = find_element_by_tag(db, page.root(), "div", &mut ctx).expect("Should find div");

    // Query CSS values for the div
    let width = db.query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    let height = db.query::<CssValueQuery>((div, "height".to_string()), &mut ctx);

    // Verify values were parsed correctly
    // 200px = 200 * 64 subpixels = 12800
    assert_eq!(
        width.subpixels_or_zero(),
        12800,
        "Width should be 200px in subpixels"
    );
    // 100px = 100 * 64 subpixels = 6400
    assert_eq!(
        height.subpixels_or_zero(),
        6400,
        "Height should be 100px in subpixels"
    );

    // Check padding was expanded from shorthand
    let padding_top = db.query::<CssValueQuery>((div, "padding-top".to_string()), &mut ctx);
    // 10px = 10 * 64 = 640 subpixels
    assert_eq!(
        padding_top.subpixels_or_zero(),
        640,
        "Padding-top should be 10px in subpixels"
    );

    // Find p element
    let p = find_element_by_tag(db, div, "p", &mut ctx).expect("Should find p");
    let margin_top = db.query::<CssValueQuery>((p, "margin-top".to_string()), &mut ctx);
    // 5px = 5 * 64 = 320 subpixels
    assert_eq!(
        margin_top.subpixels_or_zero(),
        320,
        "Margin-top should be 5px in subpixels"
    );
}

#[test]
fn test_css_value_resolution() {
    let html = r#"
        <div style="width: 100px; height: 50px; padding: 10px 20px; margin: auto">
            <span style="font-size: 16px; width: 2em">Text</span>
        </div>
    "#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    // Find div
    let div = find_element_by_tag(db, page.root(), "div", &mut ctx).expect("Should find div");

    // Test absolute pixel values
    let width = db.query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    assert_eq!(width.subpixels_or_zero(), 100 * 64);

    // Test auto keyword
    let margin = db.query::<CssValueQuery>((div, "margin-top".to_string()), &mut ctx);
    assert!(margin.is_auto(), "Margin should be auto");

    // Find span and test em units
    let span = find_element_by_tag(db, div, "span", &mut ctx).expect("Should find span");

    // Font-size is 16px = 16 * 64 = 1024 subpixels
    let font_size = db.query::<CssValueQuery>((span, "font-size".to_string()), &mut ctx);
    assert_eq!(font_size.subpixels_or_zero(), 1024);

    // Width is 2em = 2 * font-size = 2 * 1024 = 2048 subpixels
    let span_width = db.query::<CssValueQuery>((span, "width".to_string()), &mut ctx);
    assert_eq!(span_width.subpixels_or_zero(), 2048);
}

#[test]
fn test_padding_shorthand() {
    let html = r#"
        <div style="padding: 10px 20px 30px 40px">
            Content
        </div>
    "#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    // Find div
    let div = find_element_by_tag(db, page.root(), "div", &mut ctx).expect("Should find div");

    // Padding-top (10px = 640 subpixels)
    let padding_top = db.query::<CssValueQuery>((div, "padding-top".to_string()), &mut ctx);
    assert_eq!(padding_top.subpixels_or_zero(), 640);

    // Padding-right (20px = 1280 subpixels)
    let padding_right = db.query::<CssValueQuery>((div, "padding-right".to_string()), &mut ctx);
    assert_eq!(padding_right.subpixels_or_zero(), 1280);

    // Padding-bottom (30px = 1920 subpixels)
    let padding_bottom = db.query::<CssValueQuery>((div, "padding-bottom".to_string()), &mut ctx);
    assert_eq!(padding_bottom.subpixels_or_zero(), 1920);

    // Padding-left (40px = 2560 subpixels)
    let padding_left = db.query::<CssValueQuery>((div, "padding-left".to_string()), &mut ctx);
    assert_eq!(padding_left.subpixels_or_zero(), 2560);
}

#[test]
fn test_padding_longhand_values() {
    let html = r#"
        <div style="padding-top: 10px; padding-right: 20px; padding-bottom: 30px; padding-left: 40px">
            Content
        </div>
    "#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    // Find div
    let div = find_element_by_tag(db, page.root(), "div", &mut ctx).expect("Should find div");

    // Padding-top (10px = 640 subpixels)
    let padding_top = db.query::<CssValueQuery>((div, "padding-top".to_string()), &mut ctx);
    assert_eq!(padding_top.subpixels_or_zero(), 640);

    // Padding-right (20px = 1280 subpixels)
    let padding_right = db.query::<CssValueQuery>((div, "padding-right".to_string()), &mut ctx);
    assert_eq!(padding_right.subpixels_or_zero(), 1280);

    // Padding-bottom (30px = 1920 subpixels)
    let padding_bottom = db.query::<CssValueQuery>((div, "padding-bottom".to_string()), &mut ctx);
    assert_eq!(padding_bottom.subpixels_or_zero(), 1920);

    // Padding-left (40px = 2560 subpixels)
    let padding_left = db.query::<CssValueQuery>((div, "padding-left".to_string()), &mut ctx);
    assert_eq!(padding_left.subpixels_or_zero(), 2560);
}

#[test]
fn test_page_layout_computation() {
    let html = r#"
        <html>
            <body style="width: 1920px; height: 1080px">
                <div style="width: 800px; height: 600px">
                    <p>Content</p>
                </div>
            </body>
        </html>
    "#;

    let page = Page::from_html(html);

    // Compute layout
    let result = page.compute_layout();

    // Verify we get a result (even if dimensions are placeholder for now)
    assert_eq!(result.root, page.root());
}

#[test]
fn test_nested_elements_with_inheritance() {
    let html = r#"
        <div style="font-size: 20px">
            <p style="width: 5em">
                <span style="width: 2em">Text</span>
            </p>
        </div>
    "#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    let div = find_element_by_tag(db, page.root(), "div", &mut ctx).expect("Should find div");
    let div_font_size = db.query::<CssValueQuery>((div, "font-size".to_string()), &mut ctx);
    // 20px = 1280 subpixels
    assert_eq!(div_font_size.subpixels_or_zero(), 1280);

    let p = find_element_by_tag(db, div, "p", &mut ctx).expect("Should find p");
    // P's width is 5em, based on parent font-size (20px)
    // 5 * 1280 = 6400 subpixels
    let p_width = db.query::<CssValueQuery>((p, "width".to_string()), &mut ctx);
    assert_eq!(p_width.subpixels_or_zero(), 6400);

    let span = find_element_by_tag(db, p, "span", &mut ctx).expect("Should find span");
    // Span's width is 2em, also based on div's font-size (inherited)
    // 2 * 1280 = 2560 subpixels
    let span_width = db.query::<CssValueQuery>((span, "width".to_string()), &mut ctx);
    assert_eq!(span_width.subpixels_or_zero(), 2560);
}

#[test]
fn test_margin_shorthand() {
    let html = r#"<div style="margin: 10px 20px">Content</div>"#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    let div = find_element_by_tag(db, page.root(), "div", &mut ctx).expect("Should find div");

    // 2-value shorthand: vertical horizontal
    // margin-top: 10px = 640 subpixels
    let margin_top = db.query::<CssValueQuery>((div, "margin-top".to_string()), &mut ctx);
    assert_eq!(margin_top.subpixels_or_zero(), 640);

    // margin-right: 20px = 1280 subpixels
    let margin_right = db.query::<CssValueQuery>((div, "margin-right".to_string()), &mut ctx);
    assert_eq!(margin_right.subpixels_or_zero(), 1280);

    // margin-bottom should equal margin-top
    let margin_bottom = db.query::<CssValueQuery>((div, "margin-bottom".to_string()), &mut ctx);
    assert_eq!(margin_bottom.subpixels_or_zero(), 640);

    // margin-left should equal margin-right
    let margin_left = db.query::<CssValueQuery>((div, "margin-left".to_string()), &mut ctx);
    assert_eq!(margin_left.subpixels_or_zero(), 1280);
}

#[test]
fn test_gap_shorthand() {
    let html = r#"<div style="gap: 10px 20px">Content</div>"#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    let div = find_element_by_tag(db, page.root(), "div", &mut ctx).expect("Should find div");

    // row-gap: 10px = 640 subpixels
    let row_gap = db.query::<CssValueQuery>((div, "row-gap".to_string()), &mut ctx);
    assert_eq!(row_gap.subpixels_or_zero(), 640);

    // column-gap: 20px = 1280 subpixels
    let column_gap = db.query::<CssValueQuery>((div, "column-gap".to_string()), &mut ctx);
    assert_eq!(column_gap.subpixels_or_zero(), 1280);
}
