#![cfg(test)]
#![allow(
    clippy::missing_errors_doc,
    reason = "Test helpers return Result for clear propagation"
)]
#![allow(
    clippy::type_complexity,
    reason = "Test sheet helper signature is verbose by nature"
)]
#![allow(
    clippy::too_many_lines,
    reason = "Integration-style test setup is verbose"
)]
#![allow(
    clippy::missing_panics_doc,
    reason = "Assertions in tests are expected"
)]

use core::error::Error;
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

fn apply(core: &mut CoreEngine, update: DOMUpdate) -> Result<(), Box<dyn Error>> {
    core.apply_dom_update(update)?;
    Ok(())
}

fn setup_row_section(
    core: &mut CoreEngine,
    section: NodeKey,
    node_a: NodeKey,
    special: NodeKey,
    node_c: NodeKey,
) -> Result<(), Box<dyn Error>> {
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: NodeKey::ROOT,
            node: section,
            tag: "section".into(),
            pos: 0,
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: section,
            name: "class".into(),
            value: "row".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: section,
            node: node_a,
            tag: "div".into(),
            pos: 0,
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: node_a,
            name: "class".into(),
            value: "box".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: node_a,
            name: "id".into(),
            value: "a".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: section,
            node: special,
            tag: "div".into(),
            pos: 1,
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: special,
            name: "class".into(),
            value: "box".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: special,
            name: "id".into(),
            value: "special".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: section,
            node: node_c,
            tag: "div".into(),
            pos: 2,
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: node_c,
            name: "class".into(),
            value: "box".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: node_c,
            name: "id".into(),
            value: "c".into(),
        },
    )?;
    apply(core, DOMUpdate::EndOfDocument)?;
    Ok(())
}

#[test]
fn cascade_id_over_descendant_class() -> Result<(), Box<dyn Error>> {
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
    let node_a = NodeKey(2);
    let special = NodeKey(3);
    let node_c = NodeKey(4);
    setup_row_section(&mut core, section, node_a, special, node_c)?;

    let _changed = core.recompute_styles();

    let computed_section = get_computed_for(&core, section).unwrap_or_default();
    if computed_section.display != style_model::Display::Flex {
        return Err("section should be display:flex".into());
    }

    let comp_a = get_computed_for(&core, node_a).unwrap_or_default();
    let comp_special = get_computed_for(&core, special).unwrap_or_default();
    let comp_c = get_computed_for(&core, node_c).unwrap_or_default();

    if (comp_a.margin.left - 8.0).abs() >= 0.01 {
        return Err("a margin-left mismatch".into());
    }
    if (comp_a.margin.top - 8.0).abs() >= 0.01 {
        return Err("a margin-top mismatch".into());
    }
    if (comp_c.margin.left - 8.0).abs() >= 0.01 {
        return Err("c margin-left mismatch".into());
    }
    if (comp_special.margin.left - 16.0).abs() >= 0.01 {
        return Err("special margin-left mismatch".into());
    }
    if (comp_special.margin.top - 16.0).abs() >= 0.01 {
        return Err("special margin-top mismatch".into());
    }
    Ok(())
}

fn setup_wrapper_section(
    core: &mut CoreEngine,
    section: NodeKey,
    direct: NodeKey,
    nested_parent: NodeKey,
    nested: NodeKey,
) -> Result<(), Box<dyn Error>> {
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: NodeKey::ROOT,
            node: section,
            tag: "section".into(),
            pos: 0,
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: section,
            name: "class".into(),
            value: "wrapper".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: section,
            node: direct,
            tag: "div".into(),
            pos: 0,
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: direct,
            name: "class".into(),
            value: "box child".into(),
        },
    )?;
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: section,
            node: nested_parent,
            tag: "div".into(),
            pos: 1,
        },
    )?;
    apply(
        core,
        DOMUpdate::InsertElement {
            parent: nested_parent,
            node: nested,
            tag: "div".into(),
            pos: 0,
        },
    )?;
    apply(
        core,
        DOMUpdate::SetAttr {
            node: nested,
            name: "class".into(),
            value: "box desc".into(),
        },
    )?;
    apply(core, DOMUpdate::EndOfDocument)?;
    Ok(())
}

#[test]
fn combinator_child_vs_descendant() -> Result<(), Box<dyn Error>> {
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

    setup_wrapper_section(&mut core, section, direct, nested_parent, nested)?;

    let _changed = core.recompute_styles();

    let comp_direct = get_computed_for(&core, direct).unwrap_or_default();
    let comp_nested = get_computed_for(&core, nested).unwrap_or_default();
    if (comp_direct.margin.left - 4.0).abs() >= 0.01 {
        return Err("direct margin-left mismatch".into());
    }
    if (comp_nested.margin.left - 12.0).abs() >= 0.01 {
        return Err("nested margin-left mismatch".into());
    }
    Ok(())
}
