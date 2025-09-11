use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified};

#[test]
fn flex_grow_distributes_free_space_proportionally() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(5000);
    let body = NodeKey(5001);
    let container = NodeKey(5002);
    let a = NodeKey(5003);
    let b = NodeKey(5004);
    let c = NodeKey(5005);

    // DOM tree
    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: a, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: b, tag: "div".into(), pos: 1 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: c, tag: "div".into(), pos: 2 }).unwrap();

    // Styles: container is flex row; children have flex-basis:0 and grow factors 1:2:1.
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default();
    cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());

    let mut cs_container = ComputedStyle::default();
    cs_container.display = Display::Flex;
    comp.insert(container, cs_container);

    let mut cs_a = ComputedStyle::default(); cs_a.display = Display::Block; cs_a.flex_basis = SizeSpecified::Px(0.0); cs_a.flex_grow = 1.0; comp.insert(a, cs_a);
    let mut cs_b = ComputedStyle::default(); cs_b.display = Display::Block; cs_b.flex_basis = SizeSpecified::Px(0.0); cs_b.flex_grow = 2.0; comp.insert(b, cs_b);
    let mut cs_c = ComputedStyle::default(); cs_c.display = Display::Block; cs_c.flex_basis = SizeSpecified::Px(0.0); cs_c.flex_grow = 1.0; comp.insert(c, cs_c);

    l.set_computed_styles(comp);

    let _count = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let ra = rects.get(&a).expect("a rect");
    let rb = rects.get(&b).expect("b rect");
    let rc = rects.get(&c).expect("c rect");

    // Container content width used by layouter is 784px; expect widths 196, 392, 196.
    assert_eq!(ra.width, 196);
    assert_eq!(rb.width, 392);
    assert_eq!(rc.width, 196);
    // Check positioning without gaps
    assert_eq!(ra.x + ra.width, rb.x);
    assert_eq!(rb.x + rb.width, rc.x);
}

#[test]
fn flex_shrink_reduces_sizes_proportionally_with_min_constraints() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(5100);
    let body = NodeKey(5101);
    let container = NodeKey(5102);
    let a = NodeKey(5103);
    let b = NodeKey(5104);
    let c = NodeKey(5105);

    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: container, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: a, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: b, tag: "div".into(), pos: 1 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: container, node: c, tag: "div".into(), pos: 2 }).unwrap();

    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_block = ComputedStyle::default(); cs_block.display = Display::Block;
    comp.insert(html, cs_block.clone());
    comp.insert(body, cs_block.clone());

    let mut cs_container = ComputedStyle::default(); cs_container.display = Display::Flex; comp.insert(container, cs_container);

    // Each item base 300px via flex-basis, shrink equally to fit container content width (784px)
    let mut cs_a = ComputedStyle::default(); cs_a.display = Display::Block; cs_a.flex_basis = SizeSpecified::Px(300.0); cs_a.flex_shrink = 1.0; comp.insert(a, cs_a);
    let mut cs_b = ComputedStyle::default(); cs_b.display = Display::Block; cs_b.flex_basis = SizeSpecified::Px(300.0); cs_b.flex_shrink = 1.0; comp.insert(b, cs_b);
    let mut cs_c = ComputedStyle::default(); cs_c.display = Display::Block; cs_c.flex_basis = SizeSpecified::Px(300.0); cs_c.flex_shrink = 1.0; comp.insert(c, cs_c);

    l.set_computed_styles(comp);

    let _count = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let ra = rects.get(&a).expect("a rect");
    let rb = rects.get(&b).expect("b rect");
    let rc = rects.get(&c).expect("c rect");

    // Total base = 900, container 784 -> shrink by 116 -> each ~ 38 px
    assert_eq!(ra.width + rb.width + rc.width, 784);
    // Rough proportionality check: equal sizes after shrink (tolerate small rounding differences)
    assert!((ra.width - rb.width).abs() <= 1);
    assert!((rb.width - rc.width).abs() <= 1);
}
