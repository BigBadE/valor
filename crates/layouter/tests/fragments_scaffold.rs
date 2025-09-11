use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display};

#[test]
fn fragment_tree_builds_for_inline_run() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Build DOM: <html><body><div id=container><span>Hi</span> There</div></body></html>
    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(800);
    let body = NodeKey(801);
    let container = NodeKey(802);
    let span = NodeKey(803);
    let text_hi = NodeKey(804);
    let text_there = NodeKey(805);

    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: span, tag: "span".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent: span, node: text_hi, text: "Hi".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent: container, node: text_there, text: "There".into(), pos: 1 }).unwrap();

    // Computed styles: html/body/div block, span inline
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());
    comp.insert(container, cs_block.clone());

    let mut cs_span = ComputedStyle::default();
    cs_span.display = Display::Inline;
    comp.insert(span, cs_span);
    // Text nodes inherit defaults implicitly
    comp.insert(text_hi, ComputedStyle::default());
    comp.insert(text_there, ComputedStyle::default());

    l.set_computed_styles(comp);

    // Build LayoutBox tree and fragments
    let box_tree = layouter::layout::boxes::build_layout_box_tree(&l);
    // Ensure container exists in mapping
    let container_box = *box_tree.node_to_box.get(&container).expect("container box");

    let fragment_tree = layouter::layout::fragments::build_inline_fragments_for_node(&box_tree, container);
    // Expect two fragments: one for text in <span> (text_hi) and one for sibling text node (text_there)
    assert_eq!(fragment_tree.fragment_count(), 2, "expected two inline fragments in a single line");

    // Also verify grouping of inline runs returns at least one run
    let runs = layouter::layout::fragments::group_inline_runs(&box_tree, container_box);
    assert!(!runs.is_empty(), "expected at least one inline run under container");
}
