//! Minimal style system stub used by the core engine.
//! Maintains a `Stylesheet` and a small computed styles map for the root node.

use std::collections::HashMap;

use crate::{style_model, types};
use core::cmp::Ordering;
use css_color::parse_css_color;
use css_style_attr::parse_style_attribute_into_map;
use css_variables::{CustomProperties, extract_custom_properties};
use js::{DOMUpdate, NodeKey};

/// Tracks stylesheet state and a tiny computed styles cache.
pub struct StyleComputer {
    /// The active stylesheet applied to the document.
    sheet: types::Stylesheet,
    /// Snapshot of computed styles (currently only the root is populated).
    computed: HashMap<NodeKey, style_model::ComputedStyle>,
    /// Whether the last recompute changed any styles.
    style_changed: bool,
    /// Nodes whose styles changed in the last recompute.
    changed_nodes: Vec<NodeKey>,
    /// Parsed inline style attribute declarations per node (author origin).
    inline_decls_by_node: HashMap<NodeKey, HashMap<String, String>>,
    /// Extracted custom properties (variables) per node for quick lookup.
    inline_custom_props_by_node: HashMap<NodeKey, CustomProperties>,
    /// Element metadata for selector matching.
    tag_by_node: HashMap<NodeKey, String>,
    /// Element id attributes by node (used for #id selectors).
    id_by_node: HashMap<NodeKey, String>,
    /// Element class lists by node (used for .class selectors).
    classes_by_node: HashMap<NodeKey, Vec<String>>,
    /// Parent pointers for descendant/child combinator matching.
    parent_by_node: HashMap<NodeKey, NodeKey>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Declaration as CoreDecl, Origin as CoreOrigin, Rule as CoreRule, Stylesheet as CoreSheet,
    };
    use js::{DOMUpdate, NodeKey};

    #[inline]
    fn get_computed_for_test(
        sc: &StyleComputer,
        node: NodeKey,
    ) -> Option<style_model::ComputedStyle> {
        sc.computed.get(&node).cloned()
    }

    #[inline]
    fn merged_decls_for_test(sc: &StyleComputer, node: NodeKey) -> HashMap<String, String> {
        // Mirror of build_computed_from_sheet up to decl map; keep deterministic order.
        let mut props: HashMap<String, CascadedDecl> = HashMap::new();
        for rule in &sc.sheet.rules {
            let selectors = parse_selector_list(&rule.prelude);
            for selector in selectors {
                if matches_selector(node, &selector, sc) {
                    let specificity = compute_specificity(&selector);
                    for decl in &rule.declarations {
                        let entry = CascadedDecl {
                            value: decl.value.clone(),
                            important: decl.important,
                            origin: rule.origin,
                            specificity,
                            source_order: rule.source_order,
                            inline_boost: false,
                        };
                        cascade_put(&mut props, &decl.name, entry);
                    }
                }
            }
        }
        if let Some(inline) = sc.inline_decls_by_node.get(&node) {
            // Iterate deterministically over inline decl names
            let mut names: Vec<&String> = inline.keys().collect();
            names.sort();
            for name_ref in names {
                let value = inline.get(name_ref).cloned().unwrap_or_default();
                let entry = CascadedDecl {
                    value,
                    important: false,
                    origin: types::Origin::Author,
                    specificity: Specificity(1_000, 0, 0),
                    source_order: u32::MAX,
                    inline_boost: true,
                };
                cascade_put(&mut props, name_ref, entry);
            }
        }
        // Produce a sorted Vec and collect into a map for stable behavior
        let mut decls: HashMap<String, String> = HashMap::new();
        let mut pairs: Vec<(String, String)> = props
            .into_iter()
            .map(|(name, entry)| (name, entry.value))
            .collect();
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        for (name, value) in pairs {
            decls.insert(name, value);
        }
        decls
    }

    fn sheet_for_selectors(
        css_rules: Vec<(&str, Vec<(&str, &str, bool)>, CoreOrigin, u32)>,
    ) -> CoreSheet {
        let mut rules = Vec::new();
        for (prelude, decls, origin, order) in css_rules {
            let mut out: Vec<CoreDecl> = Vec::new();
            for (name, value, important) in decls {
                out.push(CoreDecl {
                    name: name.to_string(),
                    value: value.to_string(),
                    important,
                });
            }
            rules.push(CoreRule {
                origin,
                source_order: order,
                prelude: prelude.to_string(),
                declarations: out,
            });
        }
        CoreSheet {
            rules,
            origin: CoreOrigin::Author,
        }
    }

    #[test]
    fn cascade_id_over_descendant_class() {
        // .row div { margin: 8px } #special { margin: 16px } section { display: flex }
        let sheet = sheet_for_selectors(vec![
            (
                "section",
                vec![("display", "flex", false)],
                CoreOrigin::Author,
                0,
            ),
            (
                ".row div",
                vec![("margin", "8px", false)],
                CoreOrigin::Author,
                1,
            ),
            (
                "#special",
                vec![("margin", "16px", false)],
                CoreOrigin::Author,
                2,
            ),
        ]);

        let mut style_computer = StyleComputer::new();
        style_computer.replace_stylesheet(sheet);

        // Build DOM
        let section = NodeKey(1);
        let a = NodeKey(2);
        let special = NodeKey(3);
        let c = NodeKey(4);
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: NodeKey::ROOT,
            node: section,
            tag: "section".into(),
            pos: 0,
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: section,
            name: "class".into(),
            value: "row".into(),
        });
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: section,
            node: a,
            tag: "div".into(),
            pos: 0,
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: a,
            name: "class".into(),
            value: "box".into(),
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: a,
            name: "id".into(),
            value: "a".into(),
        });
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: section,
            node: special,
            tag: "div".into(),
            pos: 1,
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: special,
            name: "class".into(),
            value: "box".into(),
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: special,
            name: "id".into(),
            value: "special".into(),
        });
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: section,
            node: c,
            tag: "div".into(),
            pos: 2,
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: c,
            name: "class".into(),
            value: "box".into(),
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: c,
            name: "id".into(),
            value: "c".into(),
        });
        style_computer.apply_update(DOMUpdate::EndOfDocument);

        style_computer.recompute_dirty();
        // Verify selector matching first
        let selectors_desc = parse_selector_list(".row div");
        let first_desc = selectors_desc
            .first()
            .expect(".row div parsed to empty selector list");
        assert!(matches_selector(a, first_desc, &style_computer));
        assert!(matches_selector(c, first_desc, &style_computer));
        let selectors_id = parse_selector_list("#special");
        let first_id = selectors_id
            .first()
            .expect("#special parsed to empty selector list");
        assert!(matches_selector(special, first_id, &style_computer));
        let computed_section = get_computed_for_test(&style_computer, section).unwrap_or_default();
        assert_eq!(computed_section.display, style_model::Display::Flex);

        let map_a = merged_decls_for_test(&style_computer, a);
        assert_eq!(map_a.get("margin").map(String::as_str), Some("8px"));
        let comp_a = get_computed_for_test(&style_computer, a).unwrap_or_default();
        let comp_special = get_computed_for_test(&style_computer, special).unwrap_or_default();
        let comp_c = get_computed_for_test(&style_computer, c).unwrap_or_default();
        assert!((comp_a.margin.left - 8.0).abs() < 0.01);
        assert!((comp_a.margin.top - 8.0).abs() < 0.01);
        assert!((comp_c.margin.left - 8.0).abs() < 0.01);
        assert!((comp_special.margin.left - 16.0).abs() < 0.01);
        assert!((comp_special.margin.top - 16.0).abs() < 0.01);
    }

    #[test]
    fn combinator_child_vs_descendant() {
        // section { display:flex } .wrapper .desc { margin:12px } .wrapper > .child { margin:4px }
        let sheet = sheet_for_selectors(vec![
            (
                "section",
                vec![("display", "flex", false)],
                CoreOrigin::Author,
                0,
            ),
            (
                ".wrapper .desc",
                vec![("margin", "12px", false)],
                CoreOrigin::Author,
                1,
            ),
            (
                ".wrapper > .child",
                vec![("margin", "4px", false)],
                CoreOrigin::Author,
                2,
            ),
        ]);
        let mut style_computer = StyleComputer::new();
        style_computer.replace_stylesheet(sheet);

        let section = NodeKey(10);
        let direct = NodeKey(11);
        let nested_parent = NodeKey(12);
        let nested = NodeKey(13);
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: NodeKey::ROOT,
            node: section,
            tag: "section".into(),
            pos: 0,
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: section,
            name: "class".into(),
            value: "wrapper".into(),
        });
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: section,
            node: direct,
            tag: "div".into(),
            pos: 0,
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: direct,
            name: "class".into(),
            value: "box child".into(),
        });
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: section,
            node: nested_parent,
            tag: "div".into(),
            pos: 1,
        });
        style_computer.apply_update(DOMUpdate::InsertElement {
            parent: nested_parent,
            node: nested,
            tag: "div".into(),
            pos: 0,
        });
        style_computer.apply_update(DOMUpdate::SetAttr {
            node: nested,
            name: "class".into(),
            value: "box desc".into(),
        });
        style_computer.apply_update(DOMUpdate::EndOfDocument);

        style_computer.recompute_dirty();
        let comp_direct = get_computed_for_test(&style_computer, direct).unwrap_or_default();
        let comp_nested = get_computed_for_test(&style_computer, nested).unwrap_or_default();
        assert!((comp_direct.margin.left - 4.0).abs() < 0.01);
        assert!((comp_nested.margin.left - 12.0).abs() < 0.01);
    }
}

// (moved test helpers into tests module to avoid multiple inherent impls)

/// Insert a cascaded declaration into the property map if it wins over any existing one.
#[inline]
fn cascade_put(props: &mut HashMap<String, CascadedDecl>, name: &str, entry: CascadedDecl) {
    let should_insert = props
        .get(name)
        .is_none_or(|previous| wins_over(&entry, previous));
    if should_insert {
        props.insert(name.to_owned(), entry);
    }
}

/// Parse 4 edge values with longhand names like "{prefix}-top" in pixels.
fn parse_edges(prefix: &str, decls: &HashMap<String, String>) -> style_model::Edges {
    // Start from shorthand if present
    let mut edges = if let Some(shorthand) = decls.get(prefix) {
        let parts: Vec<&str> = shorthand
            .split(|character: char| character.is_ascii_whitespace())
            .filter(|segment| !segment.is_empty())
            .collect();
        let numbers: Vec<f32> = parts.into_iter().filter_map(parse_px).collect();
        match numbers.as_slice() {
            [one] => style_model::Edges {
                top: *one,
                right: *one,
                bottom: *one,
                left: *one,
            },
            [top, right] => style_model::Edges {
                top: *top,
                right: *right,
                bottom: *top,
                left: *right,
            },
            [top, right, bottom] => style_model::Edges {
                top: *top,
                right: *right,
                bottom: *bottom,
                left: *right,
            },
            [top, right, bottom, left] => style_model::Edges {
                top: *top,
                right: *right,
                bottom: *bottom,
                left: *left,
            },
            _ => style_model::Edges::default(),
        }
    } else {
        style_model::Edges::default()
    };
    // Longhands override shorthand sides if present
    if let Some(value) = decls.get(&format!("{prefix}-top"))
        && let Some(pixels) = parse_px(value)
    {
        edges.top = pixels;
    }
    if let Some(value) = decls.get(&format!("{prefix}-right"))
        && let Some(pixels) = parse_px(value)
    {
        edges.right = pixels;
    }
    if let Some(value) = decls.get(&format!("{prefix}-bottom"))
        && let Some(pixels) = parse_px(value)
    {
        edges.bottom = pixels;
    }
    if let Some(value) = decls.get(&format!("{prefix}-left"))
        && let Some(pixels) = parse_px(value)
    {
        edges.left = pixels;
    }
    edges
}

/// Parse layout-related keywords (display, position, z-index, overflow).
fn apply_layout_keywords(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("display") {
        computed.display = if value.eq_ignore_ascii_case("block") {
            style_model::Display::Block
        } else if value.eq_ignore_ascii_case("flex") {
            style_model::Display::Flex
        } else if value.eq_ignore_ascii_case("contents") {
            style_model::Display::Contents
        } else {
            style_model::Display::Inline
        };
    }
    if let Some(value) = decls.get("position") {
        computed.position = if value.eq_ignore_ascii_case("relative") {
            style_model::Position::Relative
        } else if value.eq_ignore_ascii_case("absolute") {
            style_model::Position::Absolute
        } else if value.eq_ignore_ascii_case("fixed") {
            style_model::Position::Fixed
        } else {
            style_model::Position::Static
        };
    }
    if let Some(value) = decls.get("z-index") {
        computed.z_index = parse_int(value);
    }
    if let Some(value) = decls.get("overflow") {
        computed.overflow = if value.eq_ignore_ascii_case("hidden") {
            style_model::Overflow::Hidden
        } else {
            style_model::Overflow::Visible
        };
    }
}

/// Parse width/height/min/max and box-sizing.
fn apply_dimensions(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(value) = decls.get("width") {
        computed.width = parse_px(value);
    }
    if let Some(value) = decls.get("height") {
        computed.height = parse_px(value);
    }
    if let Some(value) = decls.get("min-width") {
        computed.min_width = parse_px(value);
    }
    if let Some(value) = decls.get("min-height") {
        computed.min_height = parse_px(value);
    }
    if let Some(value) = decls.get("max-width") {
        computed.max_width = parse_px(value);
    }
    if let Some(value) = decls.get("max-height") {
        computed.max_height = parse_px(value);
    }
    if let Some(value) = decls.get("box-sizing") {
        computed.box_sizing = if value.eq_ignore_ascii_case("border-box") {
            style_model::BoxSizing::BorderBox
        } else {
            style_model::BoxSizing::ContentBox
        };
    }
}

/// Parse margins, paddings, and border subset.
fn apply_edges_and_borders(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    computed.margin = parse_edges("margin", decls);
    computed.padding = parse_edges("padding", decls);

    let border_widths_tmp = parse_edges("border", decls);
    computed.border_width = style_model::BorderWidths {
        top: border_widths_tmp.top,
        right: border_widths_tmp.right,
        bottom: border_widths_tmp.bottom,
        left: border_widths_tmp.left,
    };
    if let Some(value) = decls.get("border-style") {
        computed.border_style = if value.eq_ignore_ascii_case("solid") {
            style_model::BorderStyle::Solid
        } else {
            style_model::BorderStyle::None
        };
    }
}

/// Parse font-size and font-family.
fn apply_typography(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(value) = decls.get("font-size")
        && let Some(pixels) = parse_px(value)
    {
        computed.font_size = pixels;
    }
    if let Some(value) = decls.get("font-family") {
        computed.font_family = Some(value.clone());
    }
}

/// Parse flex longhands.
fn apply_flex_scalars(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(value) = decls.get("flex-grow")
        && let Ok(number) = value.trim().parse::<f32>()
    {
        computed.flex_grow = number;
    }
    if let Some(value) = decls.get("flex-shrink")
        && let Ok(number) = value.trim().parse::<f32>()
    {
        computed.flex_shrink = number;
    }
    if let Some(value) = decls.get("flex-basis") {
        computed.flex_basis = parse_px(value);
    }
}

/// Parse flex alignment properties.
fn apply_flex_alignment(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("flex-direction") {
        computed.flex_direction = if value.eq_ignore_ascii_case("column") {
            style_model::FlexDirection::Column
        } else {
            style_model::FlexDirection::Row
        };
    }
    if let Some(value) = decls.get("flex-wrap") {
        computed.flex_wrap = if value.eq_ignore_ascii_case("wrap") {
            style_model::FlexWrap::Wrap
        } else {
            style_model::FlexWrap::NoWrap
        };
    }
    if let Some(value) = decls.get("align-items") {
        computed.align_items = if value.eq_ignore_ascii_case("flex-start") {
            style_model::AlignItems::FlexStart
        } else if value.eq_ignore_ascii_case("center") {
            style_model::AlignItems::Center
        } else if value.eq_ignore_ascii_case("flex-end") {
            style_model::AlignItems::FlexEnd
        } else {
            style_model::AlignItems::Stretch
        };
    }
    if let Some(value) = decls.get("justify-content") {
        computed.justify_content = if value.eq_ignore_ascii_case("center") {
            style_model::JustifyContent::Center
        } else if value.eq_ignore_ascii_case("flex-end") {
            style_model::JustifyContent::FlexEnd
        } else if value.eq_ignore_ascii_case("space-between") {
            style_model::JustifyContent::SpaceBetween
        } else {
            style_model::JustifyContent::FlexStart
        };
    }
}

/// Build a computed style from inline declarations only, with sensible defaults.
#[inline]
fn build_computed_from_inline(decls: &HashMap<String, String>) -> style_model::ComputedStyle {
    let mut computed = style_model::ComputedStyle {
        font_size: 16.0,
        ..Default::default()
    };
    apply_layout_keywords(&mut computed, decls);
    apply_dimensions(&mut computed, decls);
    apply_edges_and_borders(&mut computed, decls);
    apply_colors(&mut computed, decls);
    apply_typography(&mut computed, decls);
    apply_flex_scalars(&mut computed, decls);
    apply_flex_alignment(&mut computed, decls);
    computed
}
