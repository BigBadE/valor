use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified, Position};

#[test]
fn fixed_positioning_relative_to_viewport() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(1100);
    let body = NodeKey(1101);
    let fixed = NodeKey(1102);

    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: fixed, tag: "div".into(), pos: 0 }).unwrap();

    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block);

    let mut cs_fixed = ComputedStyle::default();
    cs_fixed.display = Display::Block;
    cs_fixed.position = Position::Fixed;
    cs_fixed.left = Some(SizeSpecified::Px(20.0));
    cs_fixed.top = Some(SizeSpecified::Px(15.0));
    cs_fixed.width = SizeSpecified::Px(30.0);
    cs_fixed.height = SizeSpecified::Px(10.0);
    comp.insert(fixed, cs_fixed);

    l.set_computed_styles(comp);

    let _ = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let rf = rects.get(&fixed).expect("fixed rect");
    assert_eq!(rf.x, 20);
    assert_eq!(rf.y, 15);
    assert_eq!(rf.width, 30);
    assert_eq!(rf.height, 10);
}
