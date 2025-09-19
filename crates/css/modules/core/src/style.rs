//! Minimal style system stub used by the core engine.
//! Maintains a `Stylesheet` and a small computed styles map for the root node.

use std::collections::{HashMap, HashSet};

use crate::{selectors, selectors::Specificity, style_model, types};
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

/// Parse positional offsets (top/left/right/bottom) as pixels.
fn apply_offsets(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(value) = decls.get("top")
        && let Some(pixels) = parse_px(value)
    {
        computed.top = Some(pixels);
    }
    if let Some(value) = decls.get("left")
        && let Some(pixels) = parse_px(value)
    {
        computed.left = Some(pixels);
    }
    if let Some(value) = decls.get("right")
        && let Some(pixels) = parse_px(value)
    {
        computed.right = Some(pixels);
    }
    if let Some(value) = decls.get("bottom")
        && let Some(pixels) = parse_px(value)
    {
        computed.bottom = Some(pixels);
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
    // Compute line-height: 'normal' -> None; number -> number * font-size; percentage -> resolved;
    // length (px) -> as-is. Other units not yet supported in this minimal engine.
    if let Some(raw) = decls.get("line-height") {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("normal") {
            computed.line_height = None;
        } else if let Some(percent_str) = trimmed.strip_suffix('%') {
            if let Ok(percent_value) = percent_str.trim().parse::<f32>() {
                computed.line_height = Some(computed.font_size * (percent_value / 100.0));
            }
        } else if let Some(px_str) = trimmed.strip_suffix("px") {
            if let Ok(pixel_value) = px_str.trim().parse::<f32>() {
                computed.line_height = Some(pixel_value);
            }
        } else if let Ok(number) = trimmed.parse::<f32>() {
            // Unitless number multiplies element's font-size
            computed.line_height = Some(number * computed.font_size);
        }
    }
}

/// Parse flex scalars: grow, shrink, basis.
fn apply_flex_scalars(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(value) = decls.get("flex-grow")
        && let Ok(number) = value.trim().parse::<f32>()
    {
        computed.flex_grow = number;
    }
    if let Some(value) = decls.get("flex-shrink") {
        if let Ok(number) = value.trim().parse::<f32>() {
            computed.flex_shrink = number;
        }
    } else {
        // Chromium default for flex-shrink is 1
        computed.flex_shrink = 1.0;
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

/// Build a computed style from inline declarations with sensible defaults.
fn build_computed_from_inline(decls: &HashMap<String, String>) -> style_model::ComputedStyle {
    let mut computed = style_model::ComputedStyle {
        font_size: 16.0,
        ..Default::default()
    };
    // Default text color to opaque black so currentColor is defined
    if computed.color.alpha == 0 {
        computed.color = style_model::Rgba {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        };
    }
    apply_layout_keywords(&mut computed, decls);
    apply_dimensions(&mut computed, decls);
    apply_edges_and_borders(&mut computed, decls);
    apply_colors(&mut computed, decls);
    // Borders may depend on color defaults (currentColor). Finalize after colors.
    finalize_borders_after_colors(&mut computed);
    apply_typography(&mut computed, decls);
    apply_flex_scalars(&mut computed, decls);
    apply_flex_alignment(&mut computed, decls);
    apply_offsets(&mut computed, decls);
    computed
}

/// A declaration tracked during cascading with metadata used to resolve conflicts.
/// Cascaded declaration augmented with metadata for conflict resolution.
#[derive(Clone, Debug)]
struct CascadedDecl {
    /// Property value as authored (after var substitution).
    value: String,
    /// Whether the declaration was marked `!important`.
    important: bool,
    /// Rule origin (UA, user, author).
    origin: types::Origin,
    /// Winning selector specificity for the rule.
    specificity: selectors::Specificity,
    /// Rule source order used as a tie-breaker.
    source_order: u32,
    /// Inline style boost flag.
    inline_boost: bool,
}

/// Return a small integral weight for origin precedence comparisons.
const fn origin_weight(origin: types::Origin) -> u8 {
    match origin {
        types::Origin::UserAgent => 0,
        types::Origin::User => 1,
        types::Origin::Author => 2,
    }
}

/// Return true if `candidate` wins over `previous` according to CSS cascade rules.
fn wins_over(candidate: &CascadedDecl, previous: &CascadedDecl) -> bool {
    if candidate.inline_boost && !previous.inline_boost {
        return true;
    }
    if previous.inline_boost && !candidate.inline_boost {
        return false;
    }
    if candidate.important != previous.important {
        return candidate.important;
    }
    let ow_c = origin_weight(candidate.origin);
    let ow_p = origin_weight(previous.origin);
    if ow_c != ow_p {
        return ow_c > ow_p;
    }
    if candidate.specificity != previous.specificity {
        return candidate.specificity > previous.specificity;
    }
    if candidate.source_order != previous.source_order {
        return candidate.source_order > previous.source_order;
    }
    false
}

/// Check if `node` matches a simple selector (tag/id/classes).
fn matches_simple_selector(
    node: NodeKey,
    sel: &selectors::SimpleSelector,
    style_comp: &StyleComputer,
) -> bool {
    if sel.is_universal() {
        return true;
    }
    if let Some(tag) = sel.tag() {
        let tag_name = style_comp.tag_by_node.get(&node);
        if !tag_name.is_some_and(|value| value.eq_ignore_ascii_case(tag)) {
            return false;
        }
    }
    if let Some(element_id) = sel.element_id() {
        let element_id_name = style_comp.id_by_node.get(&node);
        if !element_id_name.is_some_and(|value| value.eq_ignore_ascii_case(element_id)) {
            return false;
        }
    }
    for class in sel.classes() {
        if !node_has_class(&style_comp.classes_by_node, node, class) {
            return false;
        }
    }
    true
}

/// Check whether a node matches the given parsed selector using ancestor traversal.
fn matches_selector(
    start_node: NodeKey,
    selector: &selectors::Selector,
    style_computer: &StyleComputer,
) -> bool {
    if selector.len() == 0 {
        return false;
    }
    let mut reversed = (0..selector.len()).rev().peekable();
    let mut current_node = start_node;
    loop {
        let Some(index) = reversed.next() else {
            return true;
        };
        let Some(part) = selector.part(index) else {
            return false;
        };
        if !matches_simple_selector(current_node, part.sel(), style_computer) {
            return false;
        }
        if reversed.peek().is_none() {
            return true;
        }
        let Some(prev_index) = reversed.peek().copied() else {
            return false;
        };
        let Some(prev_part) = selector.part(prev_index) else {
            return false;
        };
        let combinator = prev_part
            .combinator_to_next()
            .unwrap_or(selectors::Combinator::Descendant);
        match combinator {
            selectors::Combinator::Descendant => {
                let mut climb = current_node;
                let mut found = false;
                while let Some(parent) = style_computer.parent_by_node.get(&climb).copied() {
                    if matches_simple_selector(parent, prev_part.sel(), style_computer) {
                        current_node = parent;
                        found = true;
                        break;
                    }
                    climb = parent;
                }
                if !found {
                    return false;
                }
            }
            selectors::Combinator::Child => {
                if let Some(parent) = style_computer.parent_by_node.get(&current_node).copied() {
                    if !matches_simple_selector(parent, prev_part.sel(), style_computer) {
                        return false;
                    }
                    current_node = parent;
                } else {
                    return false;
                }
            }
        }
    }
}

// -------------- StyleComputer impl --------------
impl StyleComputer {
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

    /// Replace the current stylesheet and mark all known nodes as dirty.
    pub fn replace_stylesheet(&mut self, sheet: types::Stylesheet) {
        self.sheet = sheet;
        self.style_changed = true;
        self.changed_nodes = self.tag_by_node.keys().copied().collect();
    }

    /// Return a clone of the current computed styles snapshot.
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, style_model::ComputedStyle> {
        self.computed.clone()
    }

    /// Mirror a `DOMUpdate` into the style subsystem state.
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
            self.computed.entry(NodeKey::ROOT).or_default();
        }
        let mut any_changed = false;
        let mut visited: HashSet<NodeKey> = HashSet::new();
        let mut nodes: Vec<NodeKey> = Vec::new();
        for node_id in self.changed_nodes.drain(..) {
            if visited.insert(node_id) {
                nodes.push(node_id);
            }
        }
        if !self.tag_by_node.is_empty() && !visited.contains(&NodeKey::ROOT) {
            nodes.push(NodeKey::ROOT);
        }
        for node in nodes {
            let mut props: HashMap<String, CascadedDecl> = HashMap::new();
            for rule in &self.sheet.rules {
                apply_rule_to_props(rule, node, self, &mut props);
            }
            if let Some(inline) = self.inline_decls_by_node.get(&node) {
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
            let mut decls: HashMap<String, String> = HashMap::new();
            let mut pairs: Vec<(String, CascadedDecl)> = props.into_iter().collect();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            for (name, entry) in pairs {
                decls.insert(name, entry.value);
            }
            let computed = build_computed_from_inline(&decls);
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

/// Apply a single rule's winning declarations (if any) to the props map for node.
fn apply_rule_to_props(
    rule: &types::Rule,
    node: NodeKey,
    style_comp: &StyleComputer,
    props: &mut HashMap<String, CascadedDecl>,
) {
    let selector_list = selectors::parse_selector_list(&rule.prelude);
    for selector in selector_list {
        if !matches_selector(node, &selector, style_comp) {
            continue;
        }
        let specificity = selectors::compute_specificity(&selector);
        for decl in &rule.declarations {
            let entry = CascadedDecl {
                value: decl.value.clone(),
                important: decl.important,
                origin: rule.origin,
                specificity,
                source_order: rule.source_order,
                inline_boost: false,
            };
            cascade_put(props, &decl.name, entry);
        }
    }
}

/// Parse a CSS length in pixels; accepts unitless as px for tests. Returns None for auto/none.
fn parse_px(input: &str) -> Option<f32> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("auto") || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(px_suffix_str) = trimmed.strip_suffix("px") {
        return px_suffix_str.trim().parse::<f32>().ok();
    }
    trimmed.parse::<f32>().ok()
}

/// Parse an integer value (used for z-index).
fn parse_int(input: &str) -> Option<i32> {
    input.trim().parse::<i32>().ok()
}

/// Apply color-related properties from declarations to the computed style.
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
    // background shorthand: color-only
    if let Some(value) = decls.get("background")
        && computed.background_color.alpha == 0
    {
        for token_text in value
            .split(|character: char| character.is_ascii_whitespace())
            .filter(|text| !text.is_empty())
        {
            if let Some((red8, green8, blue8, alpha8)) = parse_css_color(token_text) {
                computed.background_color = style_model::Rgba {
                    red: red8,
                    green: green8,
                    blue: blue8,
                    alpha: alpha8,
                };
                break;
            }
        }
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
    // Default border color to currentColor if unspecified and style is not None
    if computed.border_color.alpha == 0
        && !matches!(computed.border_style, style_model::BorderStyle::None)
    {
        computed.border_color = computed.color;
    }
}

/// Return true if `node` has the given CSS class in `classes_by_node`.
#[inline]
fn node_has_class(
    classes_by_node: &HashMap<NodeKey, Vec<String>>,
    node: NodeKey,
    class_name: &str,
) -> bool {
    classes_by_node.get(&node).is_some_and(|list| {
        list.iter()
            .any(|value| value.eq_ignore_ascii_case(class_name))
    })
}

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
    let mut edges = decls
        .get(prefix)
        .map_or_else(style_model::Edges::default, |shorthand| {
            let numbers: Vec<f32> = shorthand
                .split(|character: char| character.is_ascii_whitespace())
                .filter(|segment| !segment.is_empty())
                .filter_map(parse_px)
                .collect();
            match *numbers.as_slice() {
                [one] => style_model::Edges {
                    top: one,
                    right: one,
                    bottom: one,
                    left: one,
                },
                [top, right] => style_model::Edges {
                    top,
                    right,
                    bottom: top,
                    left: right,
                },
                [top, right, bottom] => style_model::Edges {
                    top,
                    right,
                    bottom,
                    left: right,
                },
                [top, right, bottom, left] => style_model::Edges {
                    top,
                    right,
                    bottom,
                    left,
                },
                _ => style_model::Edges::default(),
            }
        });
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
        } else if value.eq_ignore_ascii_case("none") {
            style_model::Display::None
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

    // 1) Numeric-only shorthand widths for border
    let border_widths_tmp = parse_edges("border", decls);
    computed.border_width = style_model::BorderWidths {
        top: border_widths_tmp.top,
        right: border_widths_tmp.right,
        bottom: border_widths_tmp.bottom,
        left: border_widths_tmp.left,
    };

    // 2) Full border shorthand tokens
    if let Some(value) = decls.get("border") {
        apply_border_shorthand_tokens(value, computed);
    }
    // 3) Longhand border-style
    apply_border_style_longhand(decls, computed);
    // 4) Defaults for solid style
    ensure_default_solid_widths(computed);
}

#[inline]
/// Parse and apply `border` shorthand tokens: <width> <style> <color> in any order.
fn apply_border_shorthand_tokens(value: &str, computed: &mut style_model::ComputedStyle) {
    let mut width_opt: Option<f32> = None;
    let mut style_opt: Option<style_model::BorderStyle> = None;
    let mut color_opt: Option<style_model::Rgba> = None;
    for token_text in value
        .split(|character: char| character.is_ascii_whitespace())
        .filter(|text| !text.is_empty())
    {
        if width_opt.is_none()
            && let Some(px_value) = parse_px(token_text)
        {
            width_opt = Some(px_value);
            continue;
        }
        if style_opt.is_none() {
            if token_text.eq_ignore_ascii_case("solid") {
                style_opt = Some(style_model::BorderStyle::Solid);
                continue;
            }
            if token_text.eq_ignore_ascii_case("none") {
                style_opt = Some(style_model::BorderStyle::None);
                continue;
            }
        }
        if color_opt.is_none()
            && let Some((red8, green8, blue8, alpha8)) = parse_css_color(token_text)
        {
            color_opt = Some(style_model::Rgba {
                red: red8,
                green: green8,
                blue: blue8,
                alpha: alpha8,
            });
        }
    }
    if let Some(px_value) = width_opt {
        computed.border_width = style_model::BorderWidths {
            top: px_value,
            right: px_value,
            bottom: px_value,
            left: px_value,
        };
    }
    if let Some(style_value) = style_opt {
        computed.border_style = style_value;
    }
    if let Some(color_value) = color_opt {
        computed.border_color = color_value;
    }
}

#[inline]
/// Apply `border-style` longhand when present.
fn apply_border_style_longhand(
    decls: &HashMap<String, String>,
    computed: &mut style_model::ComputedStyle,
) {
    if let Some(value) = decls.get("border-style") {
        computed.border_style = if value.eq_ignore_ascii_case("solid") {
            style_model::BorderStyle::Solid
        } else {
            style_model::BorderStyle::None
        };
    }
}

#[inline]
/// Finalize border after colors are known: if widths exist, style is None, and color is present,
/// promote style to Solid; then ensure default widths for solid.
fn finalize_borders_after_colors(computed: &mut style_model::ComputedStyle) {
    let any_width = computed.border_width.top > 0.0
        || computed.border_width.right > 0.0
        || computed.border_width.bottom > 0.0
        || computed.border_width.left > 0.0;
    if any_width
        && matches!(computed.border_style, style_model::BorderStyle::None)
        && computed.border_color.alpha > 0
    {
        computed.border_style = style_model::BorderStyle::Solid;
    }
    ensure_default_solid_widths(computed);
}

#[inline]
/// Ensure a visible medium width for any side that is zero/unspecified when border-style is Solid.
fn ensure_default_solid_widths(computed: &mut style_model::ComputedStyle) {
    if matches!(computed.border_style, style_model::BorderStyle::Solid) {
        let medium = 3.0f32;
        if computed.border_width.top <= 0.0 {
            computed.border_width.top = medium;
        }
        if computed.border_width.right <= 0.0 {
            computed.border_width.right = medium;
        }
        if computed.border_width.bottom <= 0.0 {
            computed.border_width.bottom = medium;
        }
        if computed.border_width.left <= 0.0 {
            computed.border_width.left = medium;
        }
    }
}
