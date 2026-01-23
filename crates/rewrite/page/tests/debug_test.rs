//! Debug test to check if CSS inputs are being set correctly.

use rewrite_core::DependencyContext;
use rewrite_css::storage::{CssPropertyInput, CssValueQuery};
use rewrite_css::{CssValue, LengthValue};
use rewrite_html::{AttributeQuery, ChildrenQuery, TagNameQuery};
use rewrite_page::Page;

#[test]
fn debug_css_input() {
    let html = r#"<div style="padding: 10px">Content</div>"#;

    let page = Page::from_html(html);
    let db = page.database();
    let mut ctx = DependencyContext::new();

    // Find the div by searching recursively
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

    let root = page.root();
    eprintln!("Root: {:?}", root);

    if let Some(div) = find_div(db, root, &mut ctx) {
        eprintln!("Found div node: {:?}", div);

        // Check if div has style attribute
        if let Some(style) = db.query::<AttributeQuery>((div, "style".to_string()), &mut ctx) {
            eprintln!("Style attribute: {}", style);
        } else {
            eprintln!("No style attribute found");
        }

        // Try to get the input directly
        if let Some(value) = db.get_input::<CssPropertyInput>(&(div, "padding-top".to_string())) {
            eprintln!("Got padding-top input: {:?}", value);
        } else {
            eprintln!("No padding-top input found");
        }

        // Try to query it
        let padding_top = db.query::<CssValueQuery>((div, "padding-top".to_string()), &mut ctx);
        eprintln!("Query returned: {:?}", padding_top);
        eprintln!("Subpixels: {}", padding_top.subpixels_or_zero());
    } else {
        eprintln!("No div found!");
    }
}

#[test]
fn debug_manual_set() {
    use rewrite_core::Database;

    let db = Database::new();
    let node = db.create_node();

    // Manually set a CSS value
    let value = CssValue::Length(LengthValue::Px(10.0));
    db.set_input::<CssPropertyInput>((node, "padding-top".to_string()), value);

    // Query it back
    let mut ctx = DependencyContext::new();
    let result = db.query::<CssValueQuery>((node, "padding-top".to_string()), &mut ctx);

    eprintln!("Manual set result: {:?}", result);
    assert_eq!(result.subpixels_or_zero(), 640); // 10px * 64
}

#[test]
fn debug_html_attributes() {
    use rewrite_html::parse_html;

    let html = r#"<div id="test" style="padding: 10px">Content</div>"#;
    let (db, root) = parse_html(html);

    let mut ctx = DependencyContext::new();

    // Recursive function to find all elements
    fn find_all_elements(
        db: &rewrite_core::Database,
        node: rewrite_core::NodeId,
        ctx: &mut DependencyContext,
        depth: usize,
    ) {
        let indent = "  ".repeat(depth);

        if let Some(tag) = db.query::<TagNameQuery>(node, ctx) {
            eprintln!("{}Tag: {} (NodeId: {:?})", indent, tag, node);

            // Check for id and style attributes
            if let Some(id) = db.query::<AttributeQuery>((node, "id".to_string()), ctx) {
                eprintln!("{}  id=\"{}\"", indent, id);
            }
            if let Some(style) = db.query::<AttributeQuery>((node, "style".to_string()), ctx) {
                eprintln!("{}  style=\"{}\"", indent, style);
            }
        }

        // Recurse into children
        let children = db.query::<ChildrenQuery>(node, ctx);
        for &child in &children {
            find_all_elements(db, child, ctx, depth + 1);
        }
    }

    find_all_elements(&db, root, &mut ctx, 0);
}
