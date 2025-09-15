use layouter::Layouter;
use js::{NodeKey, DOMUpdate};

fn contains(kind: layouter::DirtyKind, needle: layouter::DirtyKind) -> bool { kind.contains(needle) }

#[test]
fn text_insertion_marks_node_parent_and_ancestors() {
    let mut l = Layouter::new();
    let parent = NodeKey(1);
    let text = NodeKey(2);
    // Insert a parent block and then a text child
    l.apply_update(DOMUpdate::InsertElement { parent: NodeKey::ROOT, node: parent, tag: "div".to_string(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent, node: text, text: "Hello".to_string(), pos: 0 }).unwrap();

    // Child should be STRUCTURE|GEOMETRY dirty
    let kd_child = l.dirty_kind_of(text);
    assert!(contains(kd_child, layouter::DirtyKind::STRUCTURE));
    assert!(contains(kd_child, layouter::DirtyKind::GEOMETRY));

    // Parent should be GEOMETRY dirty
    let kd_parent = l.dirty_kind_of(parent);
    assert!(contains(kd_parent, layouter::DirtyKind::GEOMETRY));

    // Root should be GEOMETRY dirty via ancestor propagation
    let kd_root = l.dirty_kind_of(NodeKey::ROOT);
    assert!(contains(kd_root, layouter::DirtyKind::GEOMETRY));

    // After compute, dirty flags are cleared
    let _ = l.compute_layout();
    assert_eq!(l.dirty_kind_of(text), layouter::DirtyKind::NONE);
}

#[test]
fn attr_set_marks_style_and_ancestors_geometry() {
    let mut l = Layouter::new();
    let node = NodeKey(10);
    l.apply_update(DOMUpdate::InsertElement { parent: NodeKey::ROOT, node, tag: "p".to_string(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::SetAttr { node, name: "id".to_string(), value: "greeting".to_string() }).unwrap();

    let kd = l.dirty_kind_of(node);
    assert!(contains(kd, layouter::DirtyKind::STYLE));
    let kd_root = l.dirty_kind_of(NodeKey::ROOT);
    assert!(contains(kd_root, layouter::DirtyKind::GEOMETRY));
}

#[test]
fn removal_marks_parent_and_ancestors_geometry() {
    let mut l = Layouter::new();
    let parent = NodeKey(20);
    let child = NodeKey(21);
    l.apply_update(DOMUpdate::InsertElement { parent: NodeKey::ROOT, node: parent, tag: "div".to_string(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent, node: child, tag: "span".to_string(), pos: 0 }).unwrap();

    // Clear any intermediate dirtiness by running a compute once
    let _ = l.compute_layout();

    // Now remove the child and assert parent + root are geometry dirty
    l.apply_update(DOMUpdate::RemoveNode { node: child }).unwrap();
    let kd_parent = l.dirty_kind_of(parent);
    assert!(contains(kd_parent, layouter::DirtyKind::STRUCTURE));
    assert!(contains(kd_parent, layouter::DirtyKind::GEOMETRY));
    let kd_root = l.dirty_kind_of(NodeKey::ROOT);
    assert!(contains(kd_root, layouter::DirtyKind::GEOMETRY));
}
