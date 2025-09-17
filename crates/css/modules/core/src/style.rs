//! Minimal style system stub used by the core engine.
//! Maintains a `Stylesheet` and a small computed styles map for the root node.

use std::collections::{HashMap, HashSet};
use std::iter::Peekable;

use crate::{style_model, types};
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
                if !matches_selector(node, &selector, sc) {
                    continue;
                }
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

/// Specificity represented as (ids, classes, tags)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Specificity(pub u32, pub u32, pub u32);

#[derive(Clone, Debug)]
struct CascadedDecl {
    value: String,
    important: bool,
    origin: types::Origin,
    specificity: Specificity,
    source_order: u32,
    // Inline style attribute boost
    inline_boost: bool,
}

#[inline]
fn origin_weight(origin: types::Origin) -> u8 {
    match origin {
        types::Origin::UserAgent => 0,
        types::Origin::User => 1,
        types::Origin::Author => 2,
    }
}

#[inline]
fn wins_over(candidate: &CascadedDecl, previous: &CascadedDecl) -> bool {
    // Inline boost wins over everything else
    if candidate.inline_boost && !previous.inline_boost {
        return true;
    }
    if previous.inline_boost && !candidate.inline_boost {
        return false;
    }

    // !important beats non-important
    if candidate.important != previous.important {
        return candidate.important;
    }

    // Higher origin wins (not relevant in tests but keeps behavior sane)
    let ow_c = origin_weight(candidate.origin);
    let ow_p = origin_weight(previous.origin);
    if ow_c != ow_p {
        return ow_c > ow_p;
    }

    // Higher specificity wins
    if candidate.specificity != previous.specificity {
        return candidate.specificity > previous.specificity;
    }

    // Later source order wins
    if candidate.source_order != previous.source_order {
        return candidate.source_order > previous.source_order;
    }

    // Otherwise, keep previous deterministically
    false
}

// -------------- Minimal selector model and parser --------------
#[derive(Clone, Debug, PartialEq, Eq)]
enum Combinator {
    Descendant,
    Child,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SimpleSelector {
    tag: Option<String>,
    element_id: Option<String>,
    classes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectorPart {
    sel: SimpleSelector,
    combinator_to_next: Option<Combinator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Selector(Vec<SelectorPart>);

#[inline]
fn compute_specificity(selector: &Selector) -> Specificity {
    let mut ids = 0u32;
    let mut classes = 0u32;
    let mut tags = 0u32;
    for part in &selector.0 {
        if part.sel.element_id.is_some() {
            ids = ids.saturating_add(1);
        }
        if !part.sel.classes.is_empty() {
            classes = classes.saturating_add(part.sel.classes.len() as u32);
        }
        if part.sel.tag.is_some() {
            tags = tags.saturating_add(1);
        }
    }
    Specificity(ids, classes, tags)
}

#[inline]
/// Consume an identifier from a character iterator.
fn consume_ident<I>(chars: &mut Peekable<I>, allow_underscore: bool) -> String
where
    I: Iterator<Item = char>,
{
    let mut out = String::new();
    while let Some(&character) = chars.peek() {
        let ok = character.is_alphanumeric()
            || character == '-'
            || (allow_underscore && character == '_');
        if !ok {
            break;
        }
        out.push(character);
        chars.next();
    }
    out
}

#[inline]
fn commit_current_part(
    parts: &mut Vec<SelectorPart>,
    current: &mut SimpleSelector,
    combinator: Combinator,
) {
    parts.push(SelectorPart {
        sel: SimpleSelector {
            tag: current.tag.take(),
            element_id: current.element_id.take(),
            classes: std::mem::take(&mut current.classes),
        },
        combinator_to_next: Some(combinator),
    });
}

#[inline]
fn parse_single_selector(selector_str: &str) -> Option<Selector> {
    let mut chars = selector_str.trim().chars().peekable();
    let mut parts: Vec<SelectorPart> = Vec::new();
    let mut current = SimpleSelector::default();
    let mut next_combinator: Option<Combinator> = None;
    let mut saw_whitespace = false;

    loop {
        // Consume whitespace as a descendant combinator boundary.
        while chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            saw_whitespace = true;
            chars.next();
        }
        if saw_whitespace {
            if current.tag.is_some() || current.element_id.is_some() || !current.classes.is_empty()
            {
                commit_current_part(&mut parts, &mut current, Combinator::Descendant);
                next_combinator = None;
            } else {
                next_combinator = Some(Combinator::Descendant);
            }
        }
        match chars.peek().copied() {
            None => break,
            Some('>') => {
                chars.next();
                // Commit current before marking combinator
                if current.tag.is_some()
                    || current.element_id.is_some()
                    || !current.classes.is_empty()
                {
                    commit_current_part(&mut parts, &mut current, Combinator::Child);
                    next_combinator = None;
                } else {
                    next_combinator = Some(Combinator::Child);
                }
            }
            Some('#') => {
                chars.next();
                current.element_id = Some(consume_ident(&mut chars, true));
            }
            Some('.') => {
                chars.next();
                current.classes.push(consume_ident(&mut chars, true));
            }
            Some(character) if character.is_alphanumeric() => {
                current.tag = Some(consume_ident(&mut chars, false));
            }
            Some(_) => {
                // Unknown; skip
                chars.next();
            }
        }
    }
    if current.tag.is_some() || current.element_id.is_some() || !current.classes.is_empty() {
        parts.push(SelectorPart {
            sel: current,
            combinator_to_next: next_combinator.take(),
        });
        // The last part should not carry a combinator to next; override to None
        if let Some(last) = parts.last_mut() {
            last.combinator_to_next = None;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(Selector(parts))
    }
}

#[inline]
fn parse_selector_list(input: &str) -> Vec<Selector> {
    input.split(',').filter_map(parse_single_selector).collect()
}

#[inline]
fn matches_simple_selector(node: NodeKey, sel: &SimpleSelector, sc: &StyleComputer) -> bool {
    if let Some(tag) = &sel.tag {
        let tag_name = sc.tag_by_node.get(&node);
        if !tag_name.is_some_and(|s| s.eq_ignore_ascii_case(tag)) {
            return false;
        }
    }
    if let Some(element_id) = &sel.element_id {
        let element_id_name = sc.id_by_node.get(&node);
        if !element_id_name.is_some_and(|s| s.eq_ignore_ascii_case(element_id)) {
            return false;
        }
    }
    for class in &sel.classes {
        if !node_has_class(&sc.classes_by_node, &node, class) {
            return false;
        }
    }
    true
}

#[inline]
fn node_has_class(
    classes_by_node: &HashMap<NodeKey, Vec<String>>,
    node: &NodeKey,
    class: &str,
) -> bool {
    classes_by_node
        .get(node)
        .map(|list| {
            list.iter()
                .any(|existing| existing.eq_ignore_ascii_case(class))
        })
        .unwrap_or(false)
}
fn matches_selector(start_node: NodeKey, selector: &Selector, sc: &StyleComputer) -> bool {
    if selector.0.is_empty() {
        return false;
    }
    let mut index: usize = selector.0.len() - 1;
    let mut current_node = start_node;
    loop {
        let part = &selector.0[index];
        if !matches_simple_selector(current_node, &part.sel, sc) {
            return false;
        }
        if index == 0 {
            return true;
        }
        let prev_index = index - 1;
        let prev = &selector.0[prev_index];
        match *prev
            .combinator_to_next
            .as_ref()
            .unwrap_or(&Combinator::Descendant)
        {
            Combinator::Descendant => {
                // Climb ancestors to find a match for prev
                let mut climb = current_node;
                let mut found = false;
                while let Some(parent) = sc.parent_by_node.get(&climb).copied() {
                    if matches_simple_selector(parent, &prev.sel, sc) {
                        current_node = parent;
                        found = true;
                        break;
                    }
                    climb = parent;
                }
                if !found {
                    return false;
                }
                if prev_index == 0 {
                    return true;
                }
                index = prev_index - 1;
            }
            Combinator::Child => {
                if let Some(parent) = sc.parent_by_node.get(&current_node).copied() {
                    if !matches_simple_selector(parent, &prev.sel, sc) {
                        return false;
                    }
                    current_node = parent;
                    if prev_index == 0 {
                        return true;
                    }
                    index = prev_index - 1;
                } else {
                    return false;
                }
            }
        }
    }
}

// -------------- StyleComputer impl --------------
impl StyleComputer {
    #[inline]
    /// Create a new `StyleComputer` with empty stylesheet and state.
    pub fn new() -> Self {
        Self {
            sheet: types::Stylesheet::default(),
            computed: HashMap::new(),
            style_changed: false,
            changed_nodes: Vec::new(),
            inline_decls_by_node: HashMap::new(),
            inline_custom_props_by_node: HashMap::new(),
            tag_by_node: HashMap::new(),
            id_by_node: HashMap::new(),
            classes_by_node: HashMap::new(),
            parent_by_node: HashMap::new(),
        }
    }

    #[inline]
    /// Replace the current stylesheet and mark all known nodes as dirty.
    pub fn replace_stylesheet(&mut self, sheet: types::Stylesheet) {
        self.sheet = sheet;
        // Mark everything dirty
        self.style_changed = true;
        // Recompute all known nodes
        self.changed_nodes = self.tag_by_node.keys().copied().collect();
    }

    #[inline]
    /// Return a clone of the current computed styles snapshot.
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, style_model::ComputedStyle> {
        // clone to keep deterministic order not required
        self.computed.clone()
    }

    /// Mirror a DOMUpdate into the style subsystem state.
    pub fn apply_update(&mut self, update: DOMUpdate) {
        match update {
            DOMUpdate::InsertElement {
                parent, node, tag, ..
            } => {
                self.tag_by_node.insert(node, tag);
                if parent == NodeKey::ROOT {
                    self.parent_by_node.remove(&node);
                } else {
                    self.parent_by_node.insert(node, parent);
                }
                self.changed_nodes.push(node);
            }
            DOMUpdate::InsertText { .. } | DOMUpdate::EndOfDocument => {}
            DOMUpdate::SetAttr { node, name, value } => {
                if name.eq_ignore_ascii_case("id") {
                    self.id_by_node.insert(node, value);
                } else if name.eq_ignore_ascii_case("class") {
                    let classes: Vec<String> = value
                        .split(|character: char| character.is_ascii_whitespace())
                        .filter(|segment| !segment.is_empty())
                        .map(|segment: &str| segment.to_owned())
                        .collect();
                    self.classes_by_node.insert(node, classes);
                } else if name.eq_ignore_ascii_case("style") {
                    let map = parse_style_attribute_into_map(&value);
                    let custom = extract_custom_properties(&map);
                    self.inline_decls_by_node.insert(node, map);
                    self.inline_custom_props_by_node.insert(node, custom);
                }
                self.changed_nodes.push(node);
            }
            DOMUpdate::RemoveNode { node } => {
                self.tag_by_node.remove(&node);
                self.id_by_node.remove(&node);
                self.classes_by_node.remove(&node);
                self.inline_decls_by_node.remove(&node);
                self.inline_custom_props_by_node.remove(&node);
                self.parent_by_node.remove(&node);
                self.computed.remove(&node);
                self.style_changed = true;
            }
        }
    }

    /// Recompute styles for nodes changed since the last pass; returns whether any styles changed.
    pub fn recompute_dirty(&mut self) -> bool {
        if self.changed_nodes.is_empty() && self.computed.is_empty() {
            // Ensure root exists at least
            self.computed.entry(NodeKey::ROOT).or_default();
        }
        let mut any_changed = false;
        let mut visited: HashSet<NodeKey> = HashSet::new();
        // Deduplicate nodes to recompute
        let mut nodes: Vec<NodeKey> = Vec::new();
        for node_id in self.changed_nodes.drain(..) {
            if visited.insert(node_id) {
                nodes.push(node_id);
            }
        }
        // Always include root if we have any elements
        if !self.tag_by_node.is_empty() && !visited.contains(&NodeKey::ROOT) {
            nodes.push(NodeKey::ROOT);
        }
        for node in nodes {
            // Merge declarations from matching rules
            let mut props: HashMap<String, CascadedDecl> = HashMap::new();
            for rule in &self.sheet.rules {
                let selectors = parse_selector_list(&rule.prelude);
                for selector in selectors {
                    if !matches_selector(node, &selector, self) {
                        continue;
                    }
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
            if let Some(inline) = self.inline_decls_by_node.get(&node) {
                // Deterministic iteration
                let mut names: Vec<&String> = inline.keys().collect();
                names.sort();
                for name in names {
                    let value = inline.get(name).cloned().unwrap_or_default();
                    let entry = CascadedDecl {
                        value,
                        important: false,
                        origin: types::Origin::Author,
                        specificity: Specificity(1_000, 0, 0),
                        source_order: u32::MAX,
                        inline_boost: true,
                    };
                    cascade_put(&mut props, name, entry);
                }
            }
            // Flatten to string map
            let mut decls: HashMap<String, String> = HashMap::new();
            // Stable order insert
            let mut pairs: Vec<(String, CascadedDecl)> = props.into_iter().collect();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            for (name, entry) in pairs {
                decls.insert(name, entry.value);
            }

            // Build computed
            let computed = build_computed_from_inline(&decls);
            // Compare against previous
            let prev = self.computed.get(&node).cloned();
            if prev.as_ref() != Some(&computed) {
                self.computed.insert(node, computed.clone());
                any_changed = true;
            }
        }
        self.style_changed = any_changed;
        any_changed
    }
}

// -------------- Value parsers --------------
/// Parse a CSS length in pixels; accepts unitless as px for tests. Returns None for auto/none.
#[inline]
fn parse_px(input: &str) -> Option<f32> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("auto") || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(px_suffix_str) = trimmed.strip_suffix("px") {
        return px_suffix_str.trim().parse::<f32>().ok();
    }
    // Accept unitless as pixels for tests
    trimmed.parse::<f32>().ok()
}

/// Parse an integer value (used for z-index).
#[inline]
fn parse_int(input: &str) -> Option<i32> {
    input.trim().parse::<i32>().ok()
}

/// Apply color-related properties from declarations to the computed style.
#[inline]
fn apply_colors(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(value) = decls.get("color")
        && let Some((red8, green8, blue8, alpha8)) = parse_css_color(value)
    {
        computed.color = style_model::Rgba {
            red: red8,
            green: green8,
            blue: blue8,
            alpha: alpha8,
        };
    }
    if let Some(value) = decls.get("background-color")
        && let Some((red8, green8, blue8, alpha8)) = parse_css_color(value)
    {
        computed.background_color = style_model::Rgba {
            red: red8,
            green: green8,
            blue: blue8,
            alpha: alpha8,
        };
    }
    if let Some(value) = decls.get("border-color")
        && let Some((red8, green8, blue8, alpha8)) = parse_css_color(value)
    {
        computed.border_color = style_model::Rgba {
            red: red8,
            green: green8,
            blue: blue8,
            alpha: alpha8,
        };
    }
}
