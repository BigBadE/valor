use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified, Overflow};

#[test]
fn overflow_hidden_clips_children_to_fixed_height() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(1200);
    let body = NodeKey(1201);
    let container = NodeKey(1202);
    let child = NodeKey(1203);

    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: child, tag: "div".into(), pos: 0 }).unwrap();

    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());

    let mut cs_container = ComputedStyle::default();
    cs_container.display = Display::Block;
    cs_container.overflow = Overflow::Hidden;
    cs_container.height = SizeSpecified::Px(30.0);
    comp.insert(container, cs_container);

    let mut cs_child = ComputedStyle::default();
    cs_child.display = Display::Block;
    cs_child.height = SizeSpecified::Px(50.0);
    comp.insert(child, cs_child);

    l.set_computed_styles(comp);

    let _ = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let rc = rects.get(&container).expect("container rect");
    let rchild = rects.get(&child).expect("child rect");

    // Child is clipped to container height and aligned at container's top
    assert_eq!(rchild.y, rc.y);
    assert_eq!(rchild.height, 30);
}
