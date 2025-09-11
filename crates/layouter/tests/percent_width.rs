use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified};

#[test]
fn percent_width_resolves_against_container_content_width() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(300);
    let body = NodeKey(301);
    let child = NodeKey(302);

    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: child, tag: "div".into(), pos: 0 }).unwrap();

    // Computed styles: child width = 50%
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    comp.insert(html, ComputedStyle::default());
    comp.insert(body, ComputedStyle::default());
    let mut cs_child = ComputedStyle::default();
    cs_child.display = Display::Block;
    cs_child.width = SizeSpecified::Percent(0.5);
    comp.insert(child, cs_child);
    l.set_computed_styles(comp);

    let _count = l.compute_layout();
    let rects = l.compute_layout_geometry();

    let r = rects.get(&child).expect("child rect");
    // container_content_width is 784 (per compute args); expect ~392 width
    assert_eq!(r.width, 392, "50% width should resolve to 392px under 784px container");
}
