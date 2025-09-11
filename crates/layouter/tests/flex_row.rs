use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified};

/// Basic flex row layout: two fixed-width children should be placed side-by-side.
#[test]
fn flex_row_places_children_horizontally() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(900);
    let body = NodeKey(901);
    let container = NodeKey(902);
    let a = NodeKey(903);
    let b = NodeKey(904);

    // DOM
    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: a, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: b, tag: "div".into(), pos: 1 }).unwrap();

    // Styles: container display:flex; children fixed widths and heights
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());

    let mut cs_flex = ComputedStyle::default();
    cs_flex.display = Display::Flex;
    comp.insert(container, cs_flex);

    let mut cs_a = ComputedStyle::default();
    cs_a.display = Display::Block;
    cs_a.width = SizeSpecified::Px(100.0);
    cs_a.height = SizeSpecified::Px(20.0);
    comp.insert(a, cs_a);

    let mut cs_b = ComputedStyle::default();
    cs_b.display = Display::Block;
    cs_b.width = SizeSpecified::Px(150.0);
    cs_b.height = SizeSpecified::Px(20.0);
    comp.insert(b, cs_b);

    l.set_computed_styles(comp);

    let _count = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let ra = rects.get(&a).expect("rect a");
    let rb = rects.get(&b).expect("rect b");

    // Expect b to be positioned immediately after a horizontally
    assert_eq!(ra.x, 0);
    assert_eq!(rb.x, ra.x + ra.width);
    assert_eq!(ra.y, rb.y);
}
