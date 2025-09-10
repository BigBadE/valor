use js::{DOMSubscriber, DOMUpdate, NodeKey};
use wgpu_renderer::Renderer;

#[test]
fn renderer_mirrors_basic_dom_updates() {
    let mut renderer = Renderer::new();

    // Build: <div id="a">Hello</div>
    let parent = NodeKey::ROOT;
    let div = NodeKey(1);
    let text = NodeKey(2);

    renderer.apply_update(DOMUpdate::InsertElement { parent, node: div, tag: "div".into(), pos: 0 }).unwrap();
    renderer.apply_update(DOMUpdate::SetAttr { node: div, name: "id".into(), value: "a".into() }).unwrap();
    renderer.apply_update(DOMUpdate::InsertText { parent: div, node: text, text: "Hello".into(), pos: 0 }).unwrap();

    // Snapshot assertions
    let snapshot = renderer.snapshot();
    assert!(snapshot.iter().any(|(k, _, _)| *k == NodeKey::ROOT));
    // div exists with a child text node
    let div_entry = snapshot.iter().find(|(k, _, _)| *k == div).cloned();
    assert!(div_entry.is_some());
    let (_k, _kind, children) = div_entry.unwrap();
    assert_eq!(children, vec![text]);

    // Remove div; both div and text should be gone from snapshot
    renderer.apply_update(DOMUpdate::RemoveNode { node: div }).unwrap();
    let snapshot2 = renderer.snapshot();
    assert!(!snapshot2.iter().any(|(k, _, _)| *k == div));
    assert!(!snapshot2.iter().any(|(k, _, _)| *k == text));
}
