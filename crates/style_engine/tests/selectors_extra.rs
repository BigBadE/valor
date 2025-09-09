use css::parser::StylesheetStreamParser;
use css::types::{Origin, Stylesheet};
use html::dom::NodeKey;
use html::dom::updating::{DOMSubscriber, DOMUpdate};
use style_engine::{ComputedStyle, Display, StyleEngine};

fn build_stylesheet(css_text: &str) -> Stylesheet {
    let mut out = Stylesheet::default();
    let mut parser = StylesheetStreamParser::new(Origin::Author, 0);
    parser.push_chunk(css_text, &mut out);
    let extra = parser.finish();
    out.rules.extend(extra.rules);
    out
}

#[test]
fn attribute_and_first_child_selectors() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Two rules: attribute equality and :first-child
    let css_text = r#"[data-kind="hero"] { color: rgb(10, 20, 30) } div:first-child { display: none }"#;
    let sheet = build_stylesheet(css_text);
    let mut engine = StyleEngine::new();
    engine.replace_stylesheet(sheet);

    // DOM: <div id=first></div><div id=second data-kind="hero"></div>
    let root = NodeKey::ROOT;
    let first = NodeKey(1001);
    let second = NodeKey(1002);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: first, tag: "div".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: second, tag: "div".into(), pos: 1 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: second, name: "data-kind".into(), value: "hero".into() }).unwrap();
    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();

    let map = engine.computed_snapshot();
    let first_cs: &ComputedStyle = map.get(&first).expect("computed for first div");
    let second_cs: &ComputedStyle = map.get(&second).expect("computed for second div");

    // first-child rule applies display:none
    assert_eq!(first_cs.display, Display::None);
    // attribute rule colors the second element
    assert_eq!(second_cs.color.red, 10);
    assert_eq!(second_cs.color.green, 20);
    assert_eq!(second_cs.color.blue, 30);
}
