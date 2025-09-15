use css::parser::StylesheetStreamParser;
use css::types::{Origin, Stylesheet};
use js::{NodeKey, DOMUpdate, DOMSubscriber};
use style_engine::StyleEngine;

fn build_stylesheet(css_text: &str) -> Stylesheet {
    let mut out = Stylesheet::default();
    let mut parser = StylesheetStreamParser::new(Origin::Author, 0);
    parser.push_chunk(css_text, &mut out);
    let extra = parser.finish();
    out.rules.extend(extra.rules);
    out
}

#[test]
fn epoch_and_dirty_perf_counters() {
    let _ = env_logger::builder().is_test(true).try_init();
    let sheet = build_stylesheet(".x { color: red }");
    let mut engine = StyleEngine::new();
    engine.replace_stylesheet(sheet);

    // Build DOM: two siblings
    let root = NodeKey::ROOT;
    let a = NodeKey(2001);
    let b = NodeKey(2002);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: a, tag: "div".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: b, tag: "div".into(), pos: 1 }).unwrap();
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "class".into(), value: "x".into() }).unwrap();
    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();

    // After initial batch, we should have recomputed some nodes
    let first_count = engine.perf_last_dirty_recompute_count();
    assert!(first_count > 0, "expected some dirty recompute on first batch, got {}", first_count);

    // Flip class on 'a' to something else and ensure recompute only touches it (and any descendants)
    engine.apply_update(DOMUpdate::SetAttr { node: a, name: "class".into(), value: "y".into() }).unwrap();
    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();
    let second_count = engine.perf_last_dirty_recompute_count();
    assert!((1..=2).contains(&second_count), "expected small dirty recompute set, got {}", second_count);

    // Replacing stylesheet bumps epoch
    let before_epoch = engine.current_rules_epoch();
    let newsheet = build_stylesheet(".y { color: blue }");
    engine.replace_stylesheet(newsheet);
    let after_epoch = engine.current_rules_epoch();
    assert!(after_epoch != before_epoch, "rules epoch should increment on stylesheet replace");
}