use style_engine::{StyleEngine, ComputedStyle, SizeSpecified};
use js::{DOMUpdate, NodeKey, DOMSubscriber};

#[test]
fn css_variables_basic_and_fallback() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Build DOM: <html><body><div id=a></div></body></html>
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT; let html = NodeKey(500); let body = NodeKey(501); let a = NodeKey(502);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: a, tag: "div".into(), pos: 0 }).unwrap();

    // Inline style: declare a custom property and consume it; and use fallback for an undefined one
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "style".into(), value: "--pad: 10px; padding: var(--pad); margin-top: var(--mt, 2px)".into() }).unwrap();
    engine.recompute_all();

    let cs: ComputedStyle = engine.computed_snapshot().get(&a).cloned().unwrap();
    assert_eq!(cs.padding.top.round() as i32, 10);
    assert_eq!(cs.padding.right.round() as i32, 10);
    assert_eq!(cs.padding.bottom.round() as i32, 10);
    assert_eq!(cs.padding.left.round() as i32, 10);
    assert_eq!(cs.margin.top.round() as i32, 2);
}

#[test]
fn css_variables_indirection_and_cycle() {
    let _ = env_logger::builder().is_test(true).try_init();
    // DOM: <div id=a></div>
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT; let html = NodeKey(600); let body = NodeKey(601); let a = NodeKey(602);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: a, tag: "div".into(), pos: 0 }).unwrap();

    // Indirection: --b uses --a, then width uses --b
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "style".into(), value: "--a: 33px; --b: var(--a); width: var(--b)".into() }).unwrap();
    engine.recompute_all();
    let cs1: ComputedStyle = engine.computed_snapshot().get(&a).cloned().unwrap();
    assert_eq!(cs1.width, SizeSpecified::Px(33.0));

    // Cyclic vars: fallback should not be used when var is defined-but-invalid; width remains Auto
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "style".into(), value: "--x: var(--y); --y: var(--x); width: var(--x, 5px)".into() }).unwrap();
    engine.recompute_all();
    let cs2: ComputedStyle = engine.computed_snapshot().get(&a).cloned().unwrap();
    assert_eq!(cs2.width, SizeSpecified::Auto);
}

#[test]
fn css_variables_for_text_properties() {
    let _ = env_logger::builder().is_test(true).try_init();
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT; let html = NodeKey(700); let body = NodeKey(701); let a = NodeKey(702);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: a, tag: "div".into(), pos: 0 }).unwrap();

    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "style".into(), value: "--fs: 20px; --lh: 2; font-size: var(--fs); line-height: var(--lh)".into() }).unwrap();
    engine.recompute_all();
    let cs: ComputedStyle = engine.computed_snapshot().get(&a).cloned().unwrap();
    assert_eq!(cs.font_size.round() as i32, 20);
    assert!((cs.line_height - 2.0).abs() < 1e-5);
}
