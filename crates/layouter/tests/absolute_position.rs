use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified, Position};

#[test]
fn absolute_positioning_with_offsets_inside_positioned_parent() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(1000);
    let body = NodeKey(1001);
    let container = NodeKey(1002);
    let abs = NodeKey(1003);

    // DOM
    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: abs, tag: "div".into(), pos: 0 }).unwrap();

    // Styles
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());

    let mut cs_container = ComputedStyle::default();
    cs_container.display = Display::Block;
    cs_container.position = Position::Relative; // establish containing block
    comp.insert(container, cs_container);

    let mut cs_abs = ComputedStyle::default();
    cs_abs.display = Display::Block;
    cs_abs.position = Position::Absolute;
    cs_abs.left = Some(SizeSpecified::Px(10.0));
    cs_abs.top = Some(SizeSpecified::Px(5.0));
    cs_abs.width = SizeSpecified::Px(40.0);
    cs_abs.height = SizeSpecified::Px(20.0);
    comp.insert(abs, cs_abs);

    l.set_computed_styles(comp);

    let _ = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let rc = rects.get(&container).expect("container rect");
    let ra = rects.get(&abs).expect("absolute child rect");

    assert_eq!(ra.x, rc.x + 10);
    assert_eq!(ra.y, rc.y + 5);
    assert_eq!(ra.width, 40);
    assert_eq!(ra.height, 20);
}
