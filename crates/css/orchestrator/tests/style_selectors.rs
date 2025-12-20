#![cfg(test)]

use core::error::Error;
use css_orchestrator::NodeKey;
use css_orchestrator::{CoreEngine, style_model, types};
use js::DOMUpdate;
use std::collections::HashMap;

/// Type alias for CSS rule specification: (selector, declarations, origin, `source_order`)
type CssRuleSpec<'rule> = (
    &'rule str,
    Vec<(&'rule str, &'rule str, bool)>,
    types::Origin,
    u32,
);

fn sheet_for_selectors(css_rules: Vec<CssRuleSpec<'_>>) -> types::Stylesheet {
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

/// Apply a DOM update to the core engine.
///
/// # Errors
/// Returns an error if the DOM update cannot be applied.
fn apply(core: &mut CoreEngine, update: DOMUpdate) -> Result<(), Box<dyn Error>> {
    core.apply_dom_update(update)?;
    Ok(())
}

/// Insert an element into the DOM.
///
/// # Errors
/// Returns an error if the insert operation fails.
fn insert_element(
    core: &mut CoreEngine,
    parent: NodeKey,
    node: NodeKey,
    tag: &str,
    pos: usize,
) -> Result<(), Box<dyn Error>> {
    apply(
        core,
        DOMUpdate::InsertElement {
            parent,
            node,
            tag: tag.into(),
            pos,
        },
    )
}

/// Set an attribute on a DOM node.
///
/// # Errors
/// Returns an error if the set attribute operation fails.
fn set_attr(
    core: &mut CoreEngine,
    node: NodeKey,
    name: &str,
    value: &str,
) -> Result<(), Box<dyn Error>> {
    apply(
        core,
        DOMUpdate::SetAttr {
            node,
            name: name.into(),
            value: value.into(),
        },
    )
}

/// Set up a test DOM structure with a section containing three child elements.
///
/// # Errors
/// Returns an error if any DOM update fails to apply.
fn setup_row_section(
    core: &mut CoreEngine,
    section: NodeKey,
    node_a: NodeKey,
    special: NodeKey,
    node_c: NodeKey,
) -> Result<(), Box<dyn Error>> {
    insert_element(core, NodeKey::ROOT, section, "section", 0)?;
    set_attr(core, section, "class", "row")?;
    insert_element(core, section, node_a, "div", 0)?;
    set_attr(core, node_a, "class", "box")?;
    set_attr(core, node_a, "id", "a")?;
    insert_element(core, section, special, "div", 1)?;
    set_attr(core, special, "class", "box")?;
    set_attr(core, special, "id", "special")?;
    insert_element(core, section, node_c, "div", 2)?;
    set_attr(core, node_c, "class", "box")?;
    set_attr(core, node_c, "id", "c")?;
    apply(core, DOMUpdate::EndOfDocument)?;
    Ok(())
}

/// Test that ID selectors cascade over descendant class selectors.
///
/// # Errors
/// Returns an error if DOM setup or style computation fails.
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

/// Set up a test DOM structure with a section containing direct and nested child elements.
///
/// # Errors
/// Returns an error if any DOM update fails to apply.
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

/// Test that child combinators (>) and descendant combinators work correctly.
///
/// # Errors
/// Returns an error if DOM setup or style computation fails.
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

/// Test that em-based font-size values are computed correctly relative to parent font size.
///
/// # Errors
/// Returns an error if DOM setup or style computation fails.
#[test]
fn font_size_em_units() -> Result<(), Box<dyn Error>> {
    let sheet = sheet_for_selectors(vec![
        (
            "h1",
            vec![("font-size", "2em", false)],
            types::Origin::UserAgent,
            0,
        ),
        (
            "h2",
            vec![("font-size", "1.5em", false)],
            types::Origin::UserAgent,
            1,
        ),
    ]);

    let mut core = CoreEngine::new();
    core.replace_stylesheet(sheet);

    let h1_node = NodeKey(1);
    let h2_node = NodeKey(2);

    insert_element(&mut core, NodeKey::ROOT, h1_node, "h1", 0)?;
    insert_element(&mut core, NodeKey::ROOT, h2_node, "h2", 1)?;
    apply(&mut core, DOMUpdate::EndOfDocument)?;

    let _changed = core.recompute_styles();

    let comp_h1 = get_computed_for(&core, h1_node).unwrap_or_default();
    let comp_h2 = get_computed_for(&core, h2_node).unwrap_or_default();

    // Default root font size is 16px
    // h1 with 2em should be 32px
    // h2 with 1.5em should be 24px
    if (comp_h1.font_size - 32.0).abs() >= 0.01 {
        return Err(format!("h1 font-size should be 32px, got {}", comp_h1.font_size).into());
    }
    if (comp_h2.font_size - 24.0).abs() >= 0.01 {
        return Err(format!("h2 font-size should be 24px, got {}", comp_h2.font_size).into());
    }
    Ok(())
}
