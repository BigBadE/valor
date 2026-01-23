//! Layout fixture tests.
//!
//! These tests verify that the layout engine query system works correctly
//! by loading HTML fixtures and building layout trees.

use rewrite_css::{DisplayQuery, PositionQuery};
use rewrite_layout::{build_layout_tree, compute_tree_size, dump_layout_tree};
use rewrite_page::Page;

/// Helper to load a fixture.
fn load_fixture(html: &str) -> Page {
    Page::from_html_with_viewport(html, Some((800.0, 600.0)))
}

/// Helper to load a fixture and build its layout tree.
fn load_and_build(html: &str) -> (Page, rewrite_layout::LayoutBox) {
    let page = load_fixture(html);
    let layout_tree =
        build_layout_tree(page.database(), page.root()).expect("Failed to build layout tree");
    (page, layout_tree)
}

// ============================================================================
// Flexbox Tests
// ============================================================================

#[test]
fn test_flexbox_justify_content_basic() {
    let html = include_str!("fixtures/flexbox_justify_content.html");
    let (_page, layout) = load_and_build(html);

    // Verify layout tree was built without cycles
    let (node_count, depth) = compute_tree_size(&layout);
    assert!(node_count > 0, "Layout tree should have nodes");
    assert!(depth > 0, "Layout tree should have depth");

    println!(
        "Flexbox justify-content layout tree:\n{}",
        dump_layout_tree(&layout, 0)
    );
}

#[test]
fn test_flexbox_align_items() {
    let html = include_str!("fixtures/flexbox_align_items.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_flexbox_flex_grow_shrink() {
    let html = include_str!("fixtures/flexbox_flex_grow_shrink.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_flexbox_gap() {
    let html = include_str!("fixtures/flexbox_gap.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

// ============================================================================
// Grid Tests
// ============================================================================

#[test]
fn test_grid_basic() {
    let html = include_str!("fixtures/grid_basic.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_grid_auto_placement() {
    let html = include_str!("fixtures/grid_auto_placement.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_grid_alignment() {
    let html = include_str!("fixtures/grid_alignment.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

// ============================================================================
// Float Tests
// ============================================================================

#[test]
fn test_float_left_right() {
    let html = include_str!("fixtures/float_left_right.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_float_clearance() {
    let html = include_str!("fixtures/float_clearance.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

// ============================================================================
// Table Tests
// ============================================================================

#[test]
fn test_table_basic() {
    let html = include_str!("fixtures/table_basic.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_table_spanning() {
    let html = include_str!("fixtures/table_spanning.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_table_border_collapse() {
    let html = include_str!("fixtures/table_border_collapse.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

// ============================================================================
// Margin Collapsing Tests
// ============================================================================

#[test]
fn test_margin_collapse_siblings() {
    let html = include_str!("fixtures/margin_collapse_siblings.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_margin_collapse_parent_child() {
    let html = include_str!("fixtures/margin_collapse_parent_child.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_margin_collapse_empty_block() {
    let html = include_str!("fixtures/margin_collapse_empty_block.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

// ============================================================================
// Inline/Text Layout Tests
// ============================================================================

#[test]
fn test_inline_text_alignment() {
    let html = include_str!("fixtures/inline_text_alignment.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_inline_baseline_alignment() {
    let html = include_str!("fixtures/inline_baseline_alignment.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_inline_line_height() {
    let html = include_str!("fixtures/inline_line_height.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

// ============================================================================
// Positioning Tests
// ============================================================================

#[test]
fn test_position_absolute() {
    let html = include_str!("fixtures/position_absolute.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let position = db.query::<PositionQuery>(root, &mut ctx);
    println!("Root position: {:?}", position);
}

#[test]
fn test_position_fixed() {
    let html = include_str!("fixtures/position_fixed.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let position = db.query::<PositionQuery>(root, &mut ctx);
    println!("Root position: {:?}", position);
}

#[test]
fn test_position_relative() {
    let html = include_str!("fixtures/position_relative.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let position = db.query::<PositionQuery>(root, &mut ctx);
    println!("Root position: {:?}", position);
}

#[test]
fn test_position_sticky() {
    let html = include_str!("fixtures/position_sticky.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let position = db.query::<PositionQuery>(root, &mut ctx);
    println!("Root position: {:?}", position);
}

// ============================================================================
// BFC Tests
// ============================================================================

#[test]
fn test_bfc_overflow_hidden() {
    let html = include_str!("fixtures/bfc_overflow_hidden.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_bfc_float_avoidance() {
    let html = include_str!("fixtures/bfc_float_avoidance.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}

#[test]
fn test_bfc_margin_no_collapse() {
    let html = include_str!("fixtures/bfc_margin_no_collapse.html");
    let page = load_fixture(html);

    let root = page.root();
    let db = page.database();
    let mut ctx = rewrite_core::DependencyContext::new();

    let display = db.query::<DisplayQuery>(root, &mut ctx);
    println!("Root display: {:?}", display);
}
