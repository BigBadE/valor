use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified};

#[test]
fn block_inline_partitioning_places_inline_in_line_and_blocks_stack() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(100);
    let body = NodeKey(101);
    let container = NodeKey(102);
    let span1 = NodeKey(103);
    let text1 = NodeKey(104);
    let span2 = NodeKey(105);
    let text2 = NodeKey(106);
    let block1 = NodeKey(107);

    // html > body > container with mixed inline and block children
    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();

    // Inline: <span>Hi</span> and <span>There</span>
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: span1, tag: "span".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent: span1, node: text1, text: "Hi".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: span2, tag: "span".into(), pos: 1 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent: span2, node: text2, text: "There".into(), pos: 0 }).unwrap();

    // Block after inline run
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: block1, tag: "div".into(), pos: 2 }).unwrap();

    // Computed styles: spans are inline, others default (block)
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());
    comp.insert(container, cs_block.clone());

    let mut cs_inline = ComputedStyle::default();
    cs_inline.display = Display::Inline;
    comp.insert(span1, cs_inline.clone());
    comp.insert(span2, cs_inline);

    // Block after inline run should be block-level with a fixed height to avoid depending on text measurement
    let mut cs_block1 = ComputedStyle::default();
    cs_block1.display = Display::Block;
    cs_block1.height = SizeSpecified::Px(40.0);
    comp.insert(block1, cs_block1);

    l.set_computed_styles(comp);

    let _count = l.compute_layout();
    let rects = l.compute_layout_geometry();

    // Inline children should produce rects with the same y (single line), and block should be below
    let r_span1 = rects.get(&span1).expect("span1 rect");
    let r_span2 = rects.get(&span2).expect("span2 rect");
    assert_eq!(r_span1.y, r_span2.y, "inline items should share the same baseline y in first line");

    let r_block1 = rects.get(&block1).expect("block1 rect");
    assert!(r_block1.y >= r_span1.y + r_span1.height, "block should be laid out below inline line box");
}
