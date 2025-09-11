use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Edges, SizeSpecified, Display};

#[test]
fn sibling_vertical_margins_collapse() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(200);
    let body = NodeKey(201);
    let a = NodeKey(202);
    let b = NodeKey(203);

    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();

    // Two sibling blocks under body
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: a, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: b, tag: "div".into(), pos: 1 }).unwrap();

    // Computed styles: set explicit heights and margins to test collapsing
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());

    let mut cs_a = ComputedStyle::default();
    cs_a.display = Display::Block;
    cs_a.height = SizeSpecified::Px(50.0);
    cs_a.margin = Edges { top: 0.0, right: 0.0, bottom: 20.0, left: 0.0 };
    comp.insert(a, cs_a);

    let mut cs_b = ComputedStyle::default();
    cs_b.display = Display::Block;
    cs_b.height = SizeSpecified::Px(50.0);
    cs_b.margin = Edges { top: 10.0, right: 0.0, bottom: 0.0, left: 0.0 };
    comp.insert(b, cs_b);

    l.set_computed_styles(comp);

    let _count = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let ra = rects.get(&a).expect("rect a");
    let rb = rects.get(&b).expect("rect b");

    // Expect b.y = a.y + a.height + max(a.margin-bottom, b.margin-top) = 0 + 50 + max(20,10) = 70
    assert_eq!(ra.y, 0, "first block starts at y=0 under body");
    assert_eq!(rb.y, ra.y + ra.height + 20, "sibling vertical margins should collapse using max");
}
