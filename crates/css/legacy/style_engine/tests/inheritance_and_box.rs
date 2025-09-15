use style_engine::{StyleEngine, ComputedStyle, SizeSpecified};
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
fn inherited_color_font_line_height_and_box_min_max_longhands() {
    let _ = env_logger::builder().is_test(true).try_init();
    // DOM: <div id=outer><div id=inner></div></div>
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT; let html = NodeKey(300); let body = NodeKey(301); let outer = NodeKey(302); let inner = NodeKey(303);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: outer, tag: "div".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: outer, node: inner, tag: "div".into(), pos: 0 }).unwrap();

    let css = r#"
        #outer { color: rgb(10,20,30); font-size: 18px; line-height: 2; }
        #inner { min-width: 20px; max-width: 40px; min-height: 5px; max-height: 15px }
    "#;
    let sheet = parse(Origin::Author, css, 0);
    engine.apply_update(DOMUpdate::SetAttr { node: outer, name: "id".into(), value: "outer".into() }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: inner, name: "id".into(), value: "inner".into() }).unwrap();
    engine.replace_stylesheet(sheet);
    engine.recompute_all();

    let snapshot = engine.computed_snapshot();
    let _cs_outer: ComputedStyle = snapshot.get(&outer).cloned().unwrap();
    let cs_inner: ComputedStyle = snapshot.get(&inner).cloned().unwrap();

    // Inheritance
    assert_eq!((cs_inner.color.red, cs_inner.color.green, cs_inner.color.blue), (10, 20, 30));
    assert_eq!(cs_inner.font_size.round() as i32, 18);
    // line-height stored as unitless multiplier; value 2 should persist
    assert!((cs_inner.line_height - 2.0).abs() < 1e-5);

    // Box min/max longhands resolution surfaced
    assert_eq!(cs_inner.min_width, Some(SizeSpecified::Px(20.0)));
    assert_eq!(cs_inner.max_width, Some(SizeSpecified::Px(40.0)));
    assert_eq!(cs_inner.min_height, Some(SizeSpecified::Px(5.0)));
    assert_eq!(cs_inner.max_height, Some(SizeSpecified::Px(15.0)));
}
