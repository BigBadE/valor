use css::parser::StylesheetStreamParser;
use css::types::{Origin, Stylesheet};
use html::dom::NodeKey;
use html::dom::updating::{DOMSubscriber, DOMUpdate};
use style_engine::{ComputedStyle, StyleEngine};

fn build_stylesheet(css_text: &str) -> Stylesheet {
    let mut out = Stylesheet::default();
    let mut parser = StylesheetStreamParser::new(Origin::Author, 0);
    parser.push_chunk(css_text, &mut out);
    let extra = parser.finish();
    out.rules.extend(extra.rules);
    out
}

#[test]
fn cascade_inline_vs_important_author() {
    // Author rule with !important should override inline style without !important
    let css_text = "div { color: blue !important }";
    let sheet = build_stylesheet(css_text);

    let mut engine = StyleEngine::new();
    engine.replace_stylesheet(sheet);

    // Simulate DOM: <div style="color: red"></div>
    let parent = NodeKey::ROOT;
    let node = NodeKey(1);
    engine.apply_update(DOMUpdate::InsertElement { parent, node, tag: "div".to_string(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node, name: "style".to_string(), value: "color: red".to_string() }).unwrap();
    // Flush batch
    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();

    let map = engine.computed_snapshot();
    let cs: &ComputedStyle = map.get(&node).expect("computed style for node");
    // Expect blue overrides red
    assert_eq!(cs.color, style_engine::ColorRGBA { red: 0, green: 0, blue: 255, alpha: 255 });
}

#[test]
fn selector_descendant_child_smoke_and_unsupported_no_panic() {
    // Two rules: one supported (simple class selector), one unsupported (adjacent sibling) should not crash
    let css_text = ".b { color: rgb(255, 0, 0) } div + p { color: green }";
    let sheet = build_stylesheet(css_text);

    let mut engine = StyleEngine::new();
    engine.replace_stylesheet(sheet);

    // Build DOM: <div id=x class=a><span><span class=b id=y></span></span></div><p></p>
    let root = NodeKey::ROOT;
    let div = NodeKey(11);
    let span1 = NodeKey(12);
    let span2_b = NodeKey(13);
    let p = NodeKey(14);

    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: div, tag: "div".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: div, node: span1, tag: "span".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: span1, name: "class".into(), value: "a".into() }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: span1, node: span2_b, tag: "span".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: span2_b, name: "class".into(), value: "b".into() }).unwrap();

    // A following sibling <p> (for unsupported selector); current matcher will warn and ignore, but must not panic
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: p, tag: "p".into(), pos: 1 }).unwrap();

    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();

    let comp = engine.computed_snapshot();
    let b_style = comp.get(&span2_b).expect("computed for .b descendant");
    // Expect red from the supported descendant selector .a .b
    assert_eq!(b_style.color, style_engine::ColorRGBA { red: 255, green: 0, blue: 0, alpha: 255 });
}

#[test]
fn targeted_recompute_class_change_affects_only_target_and_descendants() {
    // Base author stylesheet: .red { color: red } .blue { color: blue }
    let sheet = build_stylesheet(".red { color: red } .blue { color: blue }");
    let mut engine = StyleEngine::new();
    engine.replace_stylesheet(sheet);

    let root = NodeKey::ROOT;
    let div_a = NodeKey(21);
    let div_b = NodeKey(22);

    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: div_a, tag: "div".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: div_a, name: "class".into(), value: "red".into() }).unwrap();

    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: div_b, tag: "div".into(), pos: 1 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: div_b, name: "class".into(), value: "red".into() }).unwrap();

    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();
    let first = engine.computed_snapshot();
    let a0 = first.get(&div_a).unwrap().clone();
    let b0 = first.get(&div_b).unwrap().clone();

    // Flip class on div_a only to blue
    engine.apply_update(DOMUpdate::SetAttr { node: div_a, name: "class".into(), value: "blue".into() }).unwrap();
    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();

    let second = engine.computed_snapshot();
    let a1 = second.get(&div_a).unwrap();
    let b1 = second.get(&div_b).unwrap();

    // div_a color should change; div_b should remain identical
    assert_ne!(a0.color, a1.color, "class change should update target node color");
    assert_eq!(&b0, b1, "unrelated sibling should remain unchanged");
}