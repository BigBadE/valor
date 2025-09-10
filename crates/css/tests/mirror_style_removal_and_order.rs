use css::CSSMirror;
use js::{NodeKey, DOMUpdate, DOMSubscriber};

#[test]
fn style_removal_retracts_rules() {
    let mut mirror = CSSMirror::new();
    let root = NodeKey::ROOT;
    let style1 = NodeKey(100);

    // <style>div { color: red }</style>
    mirror.apply_update(DOMUpdate::InsertElement { parent: root, node: style1, tag: "style".into(), pos: 0 }).unwrap();
    mirror.apply_update(DOMUpdate::InsertText { parent: style1, node: NodeKey(101), text: "div { color: red }".into(), pos: 0 }).unwrap();
    mirror.apply_update(DOMUpdate::EndOfDocument).unwrap();

    let before = mirror.styles().clone();
    assert!(before.rules.len() >= 1, "expected at least one rule before removal");

    // Remove the style node
    mirror.apply_update(DOMUpdate::RemoveNode { node: style1 }).unwrap();
    let after = mirror.styles().clone();
    assert_eq!(after.rules.len(), 0, "expected rules to be retracted after removing <style> node");
}

#[test]
fn source_order_monotonic_interleaved() {
    let mut mirror = CSSMirror::new();
    let root = NodeKey::ROOT;
    let s1 = NodeKey(200);
    let s2 = NodeKey(201);

    // <style>div{color:red}</style>
    mirror.apply_update(DOMUpdate::InsertElement { parent: root, node: s1, tag: "style".into(), pos: 0 }).unwrap();
    mirror.apply_update(DOMUpdate::InsertText { parent: s1, node: NodeKey(202), text: "div { color: red }".into(), pos: 0 }).unwrap();

    // <style>p{color:blue}</style>
    mirror.apply_update(DOMUpdate::InsertElement { parent: root, node: s2, tag: "style".into(), pos: 1 }).unwrap();
    mirror.apply_update(DOMUpdate::InsertText { parent: s2, node: NodeKey(203), text: "p { color: blue }".into(), pos: 0 }).unwrap();

    // finalize
    mirror.apply_update(DOMUpdate::EndOfDocument).unwrap();

    let sheet = mirror.styles().clone();
    assert_eq!(sheet.rules.len(), 2, "expected two rules from two style blocks");
    let ord0 = sheet.rules[0].source_order;
    let ord1 = sheet.rules[1].source_order;
    assert!(ord0 < ord1, "expected strictly increasing source_order, got {:?} then {:?}", ord0, ord1);
}