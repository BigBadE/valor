//! Tests for CSS selector matching.

use rewrite_core::{DependencyContext, Query};
use rewrite_css::storage::value_query::{CssValueQuery, ResolvedValue};
use rewrite_html::{ChildrenQuery, TagNameQuery};
use rewrite_page::Page;

#[test]
fn test_class_selector() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                .box {
                    width: 200px;
                    height: 100px;
                }
            </style>
        </head>
        <body>
            <div class="box">Test</div>
        </body>
        </html>
    "#;

    let page = Page::from_html(html);
    let mut ctx = DependencyContext::new();

    // Find the div element
    let body = find_body(&page, &mut ctx).expect("body not found");
    let children = page.database().query::<ChildrenQuery>(body, &mut ctx);
    let div = children.first().copied().expect("div not found");

    // Check that the width from the stylesheet was applied
    let width = page
        .database()
        .query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    assert_eq!(width, ResolvedValue::Subpixels(200 * 64));

    let height = page
        .database()
        .query::<CssValueQuery>((div, "height".to_string()), &mut ctx);
    assert_eq!(height, ResolvedValue::Subpixels(100 * 64));
}

#[test]
fn test_id_selector() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                #main {
                    width: 300px;
                    padding: 20px;
                }
            </style>
        </head>
        <body>
            <div id="main">Test</div>
        </body>
        </html>
    "#;

    let page = Page::from_html(html);
    let mut ctx = DependencyContext::new();

    let body = find_body(&page, &mut ctx).expect("body not found");
    let children = page.database().query::<ChildrenQuery>(body, &mut ctx);
    let div = children.first().copied().expect("div not found");

    let width = page
        .database()
        .query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    assert_eq!(width, ResolvedValue::Subpixels(300 * 64));

    let padding = page
        .database()
        .query::<CssValueQuery>((div, "padding".to_string()), &mut ctx);
    assert_eq!(padding, ResolvedValue::Subpixels(20 * 64));
}

#[test]
fn test_type_selector() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                div {
                    width: 250px;
                }
            </style>
        </head>
        <body>
            <div>Test</div>
        </body>
        </html>
    "#;

    let page = Page::from_html(html);
    let mut ctx = DependencyContext::new();

    let body = find_body(&page, &mut ctx).expect("body not found");
    let children = page.database().query::<ChildrenQuery>(body, &mut ctx);
    let div = children.first().copied().expect("div not found");

    let width = page
        .database()
        .query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    assert_eq!(width, ResolvedValue::Subpixels(250 * 64));
}

#[test]
fn test_specificity_cascade() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                div {
                    width: 100px;
                }
                .box {
                    width: 200px;
                }
                #main {
                    width: 300px;
                }
            </style>
        </head>
        <body>
            <div id="main" class="box">Test</div>
        </body>
        </html>
    "#;

    let page = Page::from_html(html);
    let mut ctx = DependencyContext::new();

    let body = find_body(&page, &mut ctx).expect("body not found");
    let children = page.database().query::<ChildrenQuery>(body, &mut ctx);
    let div = children.first().copied().expect("div not found");

    // ID selector should win (highest specificity)
    let width = page
        .database()
        .query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    assert_eq!(width, ResolvedValue::Subpixels(300 * 64));
}

#[test]
fn test_inline_vs_stylesheet() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                #test {
                    width: 200px;
                }
            </style>
        </head>
        <body>
            <div id="test" style="width: 300px;">Test</div>
        </body>
        </html>
    "#;

    let page = Page::from_html(html);
    let mut ctx = DependencyContext::new();

    let body = find_body(&page, &mut ctx).expect("body not found");
    let children = page.database().query::<ChildrenQuery>(body, &mut ctx);
    let div = children.first().copied().expect("div not found");

    // Inline style should win over stylesheet
    let width = page
        .database()
        .query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    assert_eq!(width, ResolvedValue::Subpixels(300 * 64));
}

#[test]
fn test_multiple_classes() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                .red {
                    background-color: #ff0000;
                }
                .large {
                    width: 300px;
                }
            </style>
        </head>
        <body>
            <div class="red large">Test</div>
        </body>
        </html>
    "#;

    let page = Page::from_html(html);
    let mut ctx = DependencyContext::new();

    let body = find_body(&page, &mut ctx).expect("body not found");
    let children = page.database().query::<ChildrenQuery>(body, &mut ctx);
    let div = children.first().copied().expect("div not found");

    let width = page
        .database()
        .query::<CssValueQuery>((div, "width".to_string()), &mut ctx);
    assert_eq!(width, ResolvedValue::Subpixels(300 * 64));

    // Background color is parsed but color resolution to subpixels isn't implemented yet
    // So we just verify the width works
}

/// Helper to find the body element.
fn find_body(page: &Page, ctx: &mut DependencyContext) -> Option<rewrite_core::NodeId> {
    find_element_by_tag(page.database(), page.root(), "body", ctx)
}

/// Helper to find an element by tag name.
fn find_element_by_tag(
    db: &rewrite_core::Database,
    node: rewrite_core::NodeId,
    tag: &str,
    ctx: &mut DependencyContext,
) -> Option<rewrite_core::NodeId> {
    if let Some(tag_name) = db.query::<TagNameQuery>(node, ctx) {
        if tag_name == tag {
            return Some(node);
        }
    }

    let children = db.query::<ChildrenQuery>(node, ctx);
    for child in children {
        if let Some(found) = find_element_by_tag(db, child, tag, ctx) {
            return Some(found);
        }
    }

    None
}
