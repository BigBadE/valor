#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::type_complexity)]
#![allow(clippy::min_ident_chars)]

use css_core::NodeKey;
use css_core::{CoreEngine, style_model, types};
use js::DOMUpdate;
use std::collections::HashMap;

fn sheet_for_selectors(
    css_rules: Vec<(&str, Vec<(&str, &str, bool)>, types::Origin, u32)>,
) -> types::Stylesheet {
    let mut rules = Vec::new();
    for (prelude, decls, origin, order) in css_rules {
        let mut out = Vec::new();
        for (name, value, important) in decls {
            out.push(types::Declaration {
                name: name.to_owned(),
                value: value.to_owned(),
                important,
            });
        }
        rules.push(types::Rule {
            origin,
            source_order: order,
            prelude: prelude.to_owned(),
            declarations: out,
        });
    }
    types::Stylesheet {
        rules,
        origin: types::Origin::Author,
    }
}

fn get_computed_for(engine: &CoreEngine, node: NodeKey) -> Option<style_model::ComputedStyle> {
    let snapshot: &HashMap<NodeKey, style_model::ComputedStyle> = &engine.computed_snapshot();
    snapshot.get(&node).cloned()
}

#[test]
fn cascade_id_over_descendant_class() {
    let sheet = sheet_for_selectors(vec![
        (
            "section",
            vec![("display", "flex", false)],
            types::Origin::Author,
            0,
        ),
        (
            ".row div",
            vec![("margin", "8px", false)],
            types::Origin::Author,
            1,
        ),
        (
            "#special",
            vec![("margin", "16px", false)],
            types::Origin::Author,
            2,
        ),
    ]);

    let mut core = CoreEngine::new();
    core.replace_stylesheet(sheet);

    let section = NodeKey(1);
    let a = NodeKey(2);
    let special = NodeKey(3);
    let c = NodeKey(4);

    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: NodeKey::ROOT,
        node: section,
        tag: "section".into(),
        pos: 0,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: section,
        name: "class".into(),
        value: "row".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: section,
        node: a,
        tag: "div".into(),
        pos: 0,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: a,
        name: "class".into(),
        value: "box".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: a,
        name: "id".into(),
        value: "a".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: section,
        node: special,
        tag: "div".into(),
        pos: 1,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: special,
        name: "class".into(),
        value: "box".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: special,
        name: "id".into(),
        value: "special".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: section,
        node: c,
        tag: "div".into(),
        pos: 2,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: c,
        name: "class".into(),
        value: "box".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: c,
        name: "id".into(),
        value: "c".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::EndOfDocument).unwrap();

    let _changed = core.recompute_styles();

    let computed_section = get_computed_for(&core, section).unwrap_or_default();
    assert_eq!(computed_section.display, style_model::Display::Flex);

    let comp_a = get_computed_for(&core, a).unwrap_or_default();
    let comp_special = get_computed_for(&core, special).unwrap_or_default();
    let comp_c = get_computed_for(&core, c).unwrap_or_default();

    assert!((comp_a.margin.left - 8.0).abs() < 0.01);
    assert!((comp_a.margin.top - 8.0).abs() < 0.01);
    assert!((comp_c.margin.left - 8.0).abs() < 0.01);
    assert!((comp_special.margin.left - 16.0).abs() < 0.01);
    assert!((comp_special.margin.top - 16.0).abs() < 0.01);
}

#[test]
fn combinator_child_vs_descendant() {
    let sheet = sheet_for_selectors(vec![
        (
            "section",
            vec![("display", "flex", false)],
            types::Origin::Author,
            0,
        ),
        (
            ".wrapper .desc",
            vec![("margin", "12px", false)],
            types::Origin::Author,
            1,
        ),
        (
            ".wrapper > .child",
            vec![("margin", "4px", false)],
            types::Origin::Author,
            2,
        ),
    ]);

    let mut core = CoreEngine::new();
    core.replace_stylesheet(sheet);

    let section = NodeKey(10);
    let direct = NodeKey(11);
    let nested_parent = NodeKey(12);
    let nested = NodeKey(13);

    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: NodeKey::ROOT,
        node: section,
        tag: "section".into(),
        pos: 0,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: section,
        name: "class".into(),
        value: "wrapper".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: section,
        node: direct,
        tag: "div".into(),
        pos: 0,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: direct,
        name: "class".into(),
        value: "box child".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: section,
        node: nested_parent,
        tag: "div".into(),
        pos: 1,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::InsertElement {
        parent: nested_parent,
        node: nested,
        tag: "div".into(),
        pos: 0,
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::SetAttr {
        node: nested,
        name: "class".into(),
        value: "box desc".into(),
    })
    .unwrap();
    core.apply_dom_update(DOMUpdate::EndOfDocument).unwrap();

    let _changed = core.recompute_styles();

    let comp_direct = get_computed_for(&core, direct).unwrap_or_default();
    let comp_nested = get_computed_for(&core, nested).unwrap_or_default();
    assert!((comp_direct.margin.left - 4.0).abs() < 0.01);
    assert!((comp_nested.margin.left - 12.0).abs() < 0.01);
}
