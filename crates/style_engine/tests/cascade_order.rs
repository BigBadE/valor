use style_engine::{StyleEngine, ComputedStyle, Display};
use css::parser::StylesheetStreamParser;
use css::types::{Stylesheet, Origin};
use js::{DOMUpdate, NodeKey, DOMSubscriber};

fn stylesheet_from(origin: Origin, css: &str, base_order: u32) -> Stylesheet {
    let mut sheet = Stylesheet::default();
    let mut parser = StylesheetStreamParser::new(origin, base_order);
    parser.push_chunk(css, &mut sheet);
    let extra = parser.finish();
    let mut out = sheet;
    out.rules.extend(extra.rules);
    out
}

#[test]
fn author_beats_user_for_normal_decls_and_user_important_beats_author_important() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Build DOM: <div id="el"></div>
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT;
    let html = NodeKey(100);
    let body = NodeKey(101);
    let el = NodeKey(102);

    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: el, tag: "div".into(), pos: 0 }).unwrap();

    // UA sheet is built-in (Display defaults). Provide Author and User rules for color.
    let author = stylesheet_from(Origin::Author, "div { color: red }", 0);
    let user_normal = stylesheet_from(Origin::User, "div { color: green }", 1000);

    // Merge both into a single author snapshot we pass to replace_stylesheet
    let mut merged = Stylesheet::default();
    merged.rules.extend(author.rules);
    merged.rules.extend(user_normal.rules);
    engine.replace_stylesheet(merged);

    // Flush styles
    engine.recompute_all();
    let cs: ComputedStyle = engine.computed_snapshot().get(&el).cloned().unwrap_or_default();

    // Normal declarations: Author > User > UA
    assert_eq!(cs.color.red, 255); // red
    assert_eq!(cs.color.green, 0);

    // Now provide User !important, which should beat Author !important and normal
    let author2 = stylesheet_from(Origin::Author, "div { color: red !important }", 0);
    let user_important = stylesheet_from(Origin::User, "div { color: rgb(0, 32, 0) !important }", 1000);
    let mut merged2 = Stylesheet::default();
    merged2.rules.extend(author2.rules);
    merged2.rules.extend(user_important.rules);
    engine.replace_stylesheet(merged2);

    engine.recompute_all();
    let cs2: ComputedStyle = engine.computed_snapshot().get(&el).cloned().unwrap();
    assert_eq!((cs2.color.red, cs2.color.green, cs2.color.blue), (0, 32, 0));
}

#[test]
fn source_order_tiebreaker_for_same_specificity() {
    let _ = env_logger::builder().is_test(true).try_init();
    let mut engine = StyleEngine::new();
    let root = NodeKey::ROOT; let html = NodeKey(110); let body = NodeKey(111); let el = NodeKey(112);
    engine.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();
    engine.apply_update(DOMUpdate::InsertElement { parent: body, node: el, tag: "div".into(), pos: 0 }).unwrap();

    // Two author rules with same selector and specificity, second should win via source_order
    let a = stylesheet_from(Origin::Author, "div { display: inline }", 0);
    let b = stylesheet_from(Origin::Author, "div { display: block }", 1);
    let mut merged = Stylesheet::default();
    merged.rules.extend(a.rules);
    merged.rules.extend(b.rules);
    engine.replace_stylesheet(merged);
    engine.recompute_all();

    let cs = engine.computed_snapshot().get(&el).cloned().unwrap();
    assert_eq!(cs.display, Display::Block);
}
