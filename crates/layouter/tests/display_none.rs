use html::dom::NodeKey;
use html::dom::updating::{DOMSubscriber, DOMUpdate};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display};

#[test]
fn display_none_omits_rect() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Build a tiny layout tree: <div id=a><div id=b></div></div>
    let mut layouter = Layouter::new();
    let root = NodeKey::ROOT;
    let a = NodeKey(3001);
    let b = NodeKey(3002);
    layouter.apply_update(DOMUpdate::InsertElement { parent: root, node: a, tag: "div".into(), pos: 0 }).unwrap();
    layouter.apply_update(DOMUpdate::InsertElement { parent: a, node: b, tag: "div".into(), pos: 0 }).unwrap();

    // Provide computed styles where the outer div is display:none
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    let mut cs_a = ComputedStyle::default();
    cs_a.display = Display::None;
    comp.insert(a, cs_a);
    comp.insert(b, ComputedStyle::default());
    layouter.set_computed_styles(comp);

    let _count = layouter.compute_layout();
    let rects = layouter.compute_layout_geometry();

    // Neither 'a' nor 'b' should produce a rect because 'a' is display:none and subtree is skipped
    assert!(rects.get(&a).is_none());
    assert!(rects.get(&b).is_none());
}
