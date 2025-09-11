use style_engine::{StyleEngine, ComputedStyle};
use css::parser::StylesheetStreamParser;
use css::types::{Stylesheet, Origin};
use js::{DOMUpdate, NodeKey, DOMSubscriber};

fn parse(origin: Origin, css: &str, base_order: u32) -> Stylesheet {
    let mut sheet = Stylesheet::default();
    let mut parser = StylesheetStreamParser::new(origin, base_order);
    parser.push_chunk(css, &mut sheet);
    let extra = parser.finish();
    let mut out = sheet;
    out.rules.extend(extra.rules);
    out
}

#[test]
fn matches_type_id_class_and_attribute_equals() {
    let _ = env_logger::builder().is_test(true).try_init();
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT; let html = NodeKey(100); let body = NodeKey(101); let a = NodeKey(102);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: a, tag: "div".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "id".into(), value: "x".into() }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "class".into(), value: "c1".into() }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "data-kind".into(), value: "primary".into() }).unwrap();

    let css = r#"
        div { color: rgb(10,10,10) }
        .c1 { color: rgb(30,30,30) }
        [data-kind="primary"] { color: rgb(40,40,40) }
    "#;
    let sheet = parse(Origin::Author, css, 0);
    engine.replace_stylesheet(sheet);
    engine.recompute_all();

    let cs: ComputedStyle = engine.computed_snapshot().get(&a).cloned().unwrap();
    // last rule wins among same specificity within this test due to source order
    assert_eq!((cs.color.red, cs.color.green, cs.color.blue), (40, 40, 40));
}

#[test]
fn descendant_and_child_combinators() {
    let _ = env_logger::builder().is_test(true).try_init();
    // <div id=outer><span id=inner></span></div>
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT; let html = NodeKey(200); let body = NodeKey(201); let outer = NodeKey(202); let inner = NodeKey(203);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: outer, tag: "div".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: outer, node: inner, tag: "span".into(), pos: 0 }).unwrap();

    let css = r#"
        div span { color: rgb(1,2,3) }
        div > span { color: rgb(9,8,7) }
    "#;
    let sheet = parse(Origin::Author, css, 0);
    engine.replace_stylesheet(sheet);
    engine.recompute_all();

    let cs: ComputedStyle = engine.computed_snapshot().get(&inner).cloned().unwrap();
    // child combinator rule appears last, wins
    assert_eq!((cs.color.red, cs.color.green, cs.color.blue), (9, 8, 7));
}
