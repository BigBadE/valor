use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display};

#[test]
fn anonymous_blocks_wrap_inline_runs_in_block_container() {
    let _ = env_logger::builder().is_test(true).try_init();

    // DOM: <html><body><div id=container> A <div id=mid></div> B </div></body></html>
    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(900);
    let body = NodeKey(901);
    let container = NodeKey(902);
    let text_a = NodeKey(903);
    let mid = NodeKey(904);
    let text_b = NodeKey(905);

    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();

    l.apply_update(DOMUpdate::InsertText { parent: container, node: text_a, text: "A".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: mid, tag: "div".into(), pos: 1 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent: container, node: text_b, text: "B".into(), pos: 2 }).unwrap();

    // Computed styles: html/body/container/mid are block; text inherits defaults
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let cs_block = ComputedStyle { display: Display::Block, ..Default::default() };
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());
    comp.insert(container, cs_block.clone());
    comp.insert(mid, cs_block.clone());
    comp.insert(text_a, ComputedStyle::default());
    comp.insert(text_b, ComputedStyle::default());
    l.set_computed_styles(comp);

    let box_tree = layouter::layout::boxes::build_layout_box_tree(&l);
    let container_box_id = *box_tree.node_to_box.get(&container).expect("container box");
    let container_box = box_tree.get(container_box_id).unwrap();

    // We expect the container's children to be: [AnonymousBlock, Block(mid), AnonymousBlock]
    assert!(container_box.children.len() >= 3, "expected at least three child boxes under container");
    use layouter::layout::boxes::LayoutBoxKind;
    let first_kind = &box_tree.get(container_box.children[0]).unwrap().kind;
    let second_kind = &box_tree.get(container_box.children[1]).unwrap().kind;
    let third_kind = &box_tree.get(container_box.children[2]).unwrap().kind;

    assert!(matches!(first_kind, LayoutBoxKind::AnonymousBlock), "first child should be AnonymousBlock wrapping leading inline run");
    assert!(matches!(second_kind, LayoutBoxKind::Block { .. } | LayoutBoxKind::InlineElement { .. }), "second child should be a real element box for mid block");
    assert!(matches!(third_kind, LayoutBoxKind::AnonymousBlock), "third child should be AnonymousBlock wrapping trailing inline run");
}
