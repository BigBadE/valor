//! Minimal CSS style engine facade used by tests and higher layers.
//!
//! This module now delegates style computation to the real `css_core` engine by
//! replaying the current layout snapshot (tags, structure) and attributes
//! (id/class/style) into `css_core::CoreEngine`. The resulting computed styles
//! are mapped into this crate's `ComputedStyle` for layouter and tests.

use anyhow::Result;
use core::mem::take;
use css::types::{Origin as CssOrigin, Stylesheet as CssStylesheet};
use css_core::style_model::{
    AlignItems as CoreAlignItems, BorderStyle as CoreBorderStyle, BorderWidths as CoreBorderWidths,
    ComputedStyle as CoreComputedStyle, Display as CoreDisplay, Edges as CoreEdges,
    Overflow as CoreOverflow, Position as CorePosition, Rgba as CoreRgba,
};
use css_core::{
    CoreEngine as CoreCssEngine,
    types::{
        Declaration as CoreDecl, Origin as CoreOrigin, Rule as CoreRule, Stylesheet as CoreSheet,
    },
};
use js::DOMUpdate::{EndOfDocument, InsertElement, InsertText, RemoveNode, SetAttr};
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

/// Apply a small subset of attributes (id, class, style) for `child`.
#[inline]
fn apply_attrs_for_child(core: &mut CoreCssEngine, child: NodeKey, map: &HashMap<String, String>) {
    for key_name in ["id", "class", "style"] {
        if let Some(value) = map.get(key_name) {
            let _ignored = core.apply_dom_update(SetAttr {
                node: child,
                name: key_name.to_owned(),
                value: value.clone(),
            });
        }
    }
}

// helper moved into main impl block below to avoid multiple inherent impls

#[cfg(test)]
mod tests {
    #![allow(
        clippy::missing_panics_doc,
        reason = "simple unit tests may assert/panic"
    )]
    #![allow(
        clippy::type_complexity,
        reason = "test helper signature is verbose by nature"
    )]
    #![allow(
        clippy::too_many_lines,
        reason = "integration-style test is intentionally explicit"
    )]
    use super::*;
    use css::types::{Declaration as CssDecl, Rule as CssRule};

    fn build_sheet(rules: Vec<(&str, Vec<(&str, &str, bool)>, CssOrigin, u32)>) -> CssStylesheet {
        let mut out = CssStylesheet::default();
        for (prelude, decls, origin, source_order) in rules {
            out.rules.push(CssRule {
                prelude: prelude.to_owned(),
                declarations: decls
                    .into_iter()
                    .map(|(name, value, important)| CssDecl {
                        name: name.to_owned(),
                        value: value.to_owned(),
                        important,
                    })
                    .collect(),
                origin,
                source_order,
            });
        }
        out
    }

    #[test]
    fn replay_and_stylesheet_apply_row_desc() {
        let mut engine = StyleEngine::new();
        let sheet = build_sheet(vec![
            (
                "section",
                vec![("display", "flex", false)],
                CssOrigin::Author,
                0,
            ),
            (
                ".row div",
                vec![("margin", "8px", false)],
                CssOrigin::Author,
                1,
            ),
            (
                "#special",
                vec![("margin", "16px", false)],
                CssOrigin::Author,
                2,
            ),
        ]);
        engine.replace_stylesheet(sheet);

        // Build a small DOM snapshot: section > [#a.box, #special.box, #c.box]
        let section = NodeKey(1);
        let node_a = NodeKey(2);
        let special = NodeKey(3);
        let node_c = NodeKey(4);

        let mut tags_by_key: HashMap<NodeKey, String> = HashMap::new();
        tags_by_key.insert(section, "section".to_owned());
        tags_by_key.insert(node_a, "div".to_owned());
        tags_by_key.insert(special, "div".to_owned());
        tags_by_key.insert(node_c, "div".to_owned());

        let mut element_children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        element_children.insert(NodeKey::ROOT, vec![section]);
        element_children.insert(section, vec![node_a, special, node_c]);

        let mut attrs: HashMap<NodeKey, HashMap<String, String>> = HashMap::new();
        attrs
            .entry(section)
            .or_default()
            .insert("class".to_owned(), "row".to_owned());
        attrs
            .entry(node_a)
            .or_default()
            .insert("class".to_owned(), "box".to_owned());
        attrs
            .entry(node_a)
            .or_default()
            .insert("id".to_owned(), "a".to_owned());
        attrs
            .entry(special)
            .or_default()
            .insert("class".to_owned(), "box".to_owned());
        attrs
            .entry(special)
            .or_default()
            .insert("id".to_owned(), "special".to_owned());
        attrs
            .entry(node_c)
            .or_default()
            .insert("class".to_owned(), "box".to_owned());
        attrs
            .entry(node_c)
            .or_default()
            .insert("id".to_owned(), "c".to_owned());

        engine.rebuild_from_layout_snapshot(&tags_by_key, &element_children, &attrs);
        engine.recompute_dirty();

        let snapshot = engine.computed_snapshot();
        let comp_section = snapshot.get(&section).cloned().unwrap_or_default();
        assert!(matches!(comp_section.display, Display::Flex));

        let comp_a = snapshot.get(&node_a).cloned().unwrap_or_default();
        let comp_special = snapshot.get(&special).cloned().unwrap_or_default();
        let comp_c = snapshot.get(&node_c).cloned().unwrap_or_default();
        assert!((comp_a.margin.left - 8.0).abs() < 0.01);
        assert!((comp_a.margin.top - 8.0).abs() < 0.01);
        assert!((comp_c.margin.left - 8.0).abs() < 0.01);
        assert!((comp_special.margin.left - 16.0).abs() < 0.01);
        assert!((comp_special.margin.top - 16.0).abs() < 0.01);
    }
}

#[cfg(test)]
mod extra_tests {
    #![allow(
        clippy::missing_panics_doc,
        reason = "simple unit tests may assert/panic"
    )]
    #![allow(
        clippy::type_complexity,
        reason = "test helper signature is verbose by nature"
    )]
    use super::*;
    use css::types::{Declaration as CssDecl, Rule as CssRule};

    #[test]
    fn ancestor_only_class_triggers_descendant_match() {
        let mut engine = StyleEngine::new();
        let mut sheet = CssStylesheet::default();
        sheet.rules.push(CssRule {
            prelude: ".row div".to_owned(),
            declarations: vec![CssDecl {
                name: "margin".to_owned(),
                value: "8px".to_owned(),
                important: false,
            }],
            origin: CssOrigin::Author,
            source_order: 0,
        });
        engine.replace_stylesheet(sheet);

        // Build a small DOM snapshot: section > [.row > #child1, #child2]
        let section = NodeKey(1);
        let child1 = NodeKey(2);
        let child2 = NodeKey(3);

        let mut tags_by_key: HashMap<NodeKey, String> = HashMap::new();
        tags_by_key.insert(section, "section".to_owned());
        tags_by_key.insert(child1, "div".to_owned());
        tags_by_key.insert(child2, "div".to_owned());

        let mut element_children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        element_children.insert(NodeKey::ROOT, vec![section]);
        element_children.insert(section, vec![child1, child2]);

        let mut attrs: HashMap<NodeKey, HashMap<String, String>> = HashMap::new();
        attrs
            .entry(section)
            .or_default()
            .insert("class".to_owned(), "row".to_owned());

        engine.rebuild_from_layout_snapshot(&tags_by_key, &element_children, &attrs);
        engine.recompute_dirty();

        let snapshot = engine.computed_snapshot();
        let comp_child1 = snapshot.get(&child1).cloned().unwrap_or_default();
        let comp_child2 = snapshot.get(&child2).cloned().unwrap_or_default();
        assert!((comp_child1.margin.left - 8.0).abs() < 0.01);
        assert!((comp_child2.margin.left - 8.0).abs() < 0.01);
    }
}

/// Recursively replay a cached layout snapshot into the core engine.
#[inline]
fn replay_node(
    core: &mut CoreCssEngine,
    tags_by_key: &HashMap<NodeKey, String>,
    attrs: &HashMap<NodeKey, HashMap<String, String>>,
    element_children: &HashMap<NodeKey, Vec<NodeKey>>,
    parent: NodeKey,
) {
    let Some(children) = element_children.get(&parent) else {
        return;
    };
    for child in children {
        let tag = tags_by_key
            .get(child)
            .cloned()
            .unwrap_or_else(|| String::from("div"));
        let _ignored_ok = core
            .apply_dom_update(InsertElement {
                parent,
                node: *child,
                tag,
                pos: 0,
            })
            .is_ok();
        if let Some(map) = attrs.get(child) {
            apply_attrs_for_child(core, *child, map);
        }
        replay_node(core, tags_by_key, attrs, element_children, *child);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BorderWidths {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Display {
    #[default]
    Block,
    Inline,
    None,
    Flex,
    InlineFlex,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
    Auto,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    Center,
    FlexStart,
    FlexEnd,
    Baseline,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum LengthOrAuto {
    #[default]
    Auto,
    Pixels(f32),
    Percent(f32),
}

// Align with tests expecting `SizeSpecified`
pub type SizeSpecified = LengthOrAuto;

#[derive(Clone, Debug, Default)]
pub struct ComputedStyle {
    pub color: Rgba,
    pub background_color: Rgba,
    pub border_width: Edges,
    pub border_style: BorderStyle,
    pub border_color: Rgba,
    pub display: Display,
    pub margin: Edges,
    pub padding: Edges,
    pub flex_basis: LengthOrAuto,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub align_items: AlignItems,
    pub font_size: f32,
    pub overflow: Overflow,
    pub position: Position,
    pub z_index: Option<i32>,
    // Box/dimensions
    pub box_sizing: BoxSizing,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    // Offsets (for positioned layout)
    pub top: Option<f32>,
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    // Floats
    pub float: Float,
    pub clear: Clear,
}

#[derive(Default)]
pub struct StyleEngine {
    /// Per-node attribute map used by the style system.
    attrs: HashMap<NodeKey, HashMap<String, String>>,
    /// The active stylesheet currently applied to the document.
    stylesheet: CssStylesheet,
    /// The latest computed styles keyed by node.
    computed: HashMap<NodeKey, ComputedStyle>,
    /// Tracks whether any style changes occurred since the last snapshot.
    style_changed: bool,
    /// Nodes whose styles changed since the last recomputation.
    changed_nodes: Vec<NodeKey>,
    /// Cached last-known element tags per node from layout snapshot.
    tags_by_key: HashMap<NodeKey, String>,
    /// Cached element-children relationships from layout snapshot.
    element_children: HashMap<NodeKey, Vec<NodeKey>>,
    /// Internal core engine used for real CSS computation.
    core: CoreCssEngine,
}

impl StyleEngine {
    #[inline]
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
            stylesheet: CssStylesheet::default(),
            computed: HashMap::new(),
            style_changed: false,
            changed_nodes: Vec::new(),
            tags_by_key: HashMap::new(),
            element_children: HashMap::new(),
            core: CoreCssEngine::new(),
        }
    }

    #[inline]
    pub fn sync_attrs_from_map(&mut self, map: &HashMap<NodeKey, HashMap<String, String>>) {
        self.attrs.clone_from(map);
    }

    #[inline]
    pub fn rebuild_from_layout_snapshot(
        &mut self,
        tags_by_key: &HashMap<NodeKey, String>,
        element_children: &HashMap<NodeKey, Vec<NodeKey>>,
        layout_attrs: &HashMap<NodeKey, HashMap<String, String>>,
    ) {
        // Cache structure and attributes for the next recompute.
        self.tags_by_key.clone_from(tags_by_key);
        self.element_children.clone_from(element_children);
        self.attrs.clone_from(layout_attrs);
    }

    #[inline]
    pub fn replace_stylesheet(&mut self, stylesheet: CssStylesheet) {
        self.stylesheet = stylesheet;
        // Mark styles as needing recomputation on stylesheet replacement.
        self.style_changed = true;
    }

    #[inline]
    pub const fn force_full_restyle(&mut self) {
        // Request a full restyle on the next recompute.
        self.style_changed = true;
    }

    #[inline]
    pub fn recompute_dirty(&mut self) {
        // Rebuild the core engine from the cached layout snapshot and attributes.
        self.core = CoreCssEngine::new();

        replay_node(
            &mut self.core,
            &self.tags_by_key,
            &self.attrs,
            &self.element_children,
            NodeKey::ROOT,
        );
        if self.core.apply_dom_update(EndOfDocument).is_err() {
            // Non-fatal in tests; continue recompute path.
        }

        // Replace stylesheet in core
        self.core
            .replace_stylesheet(map_sheet_to_core(&self.stylesheet));

        // Compute styles in core and map back
        let _styles_changed = self.core.recompute_styles();
        let core_snapshot = self.core.computed_snapshot();
        let mut mapped: HashMap<NodeKey, ComputedStyle> = HashMap::new();
        let mut pairs: Vec<(NodeKey, CoreComputedStyle)> = core_snapshot.into_iter().collect();
        pairs.sort_by_key(|&(key, _)| key.0);
        for (key, core_style) in pairs {
            mapped.insert(key, map_core_to_public(&core_style));
        }
        self.style_changed = true;
        self.changed_nodes = mapped.keys().copied().collect();
        self.computed = mapped;
    }
    #[inline]
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, ComputedStyle> {
        self.computed.clone()
    }
    #[inline]
    pub const fn take_and_clear_style_changed(&mut self) -> bool {
        let was_style_changed = self.style_changed;
        self.style_changed = false;
        was_style_changed
    }
    #[inline]
    pub fn take_changed_nodes(&mut self) -> Vec<NodeKey> {
        take(&mut self.changed_nodes)
    }

    /// Mark `node` and all its descendants as style-changed.
    #[inline]
    fn mark_subtree_changed(&mut self, node: NodeKey) {
        let mut stack: Vec<NodeKey> = vec![node];
        while let Some(current) = stack.pop() {
            self.changed_nodes.push(current);
            if let Some(children) = self.element_children.get(&current) {
                for child in children {
                    stack.push(*child);
                }
            }
        }
        self.style_changed = true;
    }
}

impl DOMSubscriber for StyleEngine {
    #[inline]
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        match update {
            InsertElement { node, .. } => {
                // Ensure an entry exists for this node in case attrs are set before snapshot
                self.attrs.entry(node).or_default();
            }
            SetAttr { node, name, value } => {
                // Track only relevant style-affecting attributes
                let key = name.to_ascii_lowercase();
                if key == "id" || key == "class" || key == "style" {
                    let entry = self.attrs.entry(node).or_default();
                    entry.insert(key, value);
                    // Mark this node and all descendants as changed to handle descendant selectors
                    self.mark_subtree_changed(node);
                }
            }
            RemoveNode { node } => {
                if self.attrs.remove(&node).is_some() {
                    self.style_changed = true;
                }
            }
            EndOfDocument | InsertText { .. } => { /* ignore */ }
        }
        Ok(())
    }
}

// ===== Mapping helpers =====

/// Map public `css::types::Stylesheet` to core `CoreSheet`.
fn map_sheet_to_core(sheet: &CssStylesheet) -> CoreSheet {
    let mut rules_out: Vec<CoreRule> = Vec::new();
    for rule_pub in &sheet.rules {
        let mut decls: Vec<CoreDecl> = Vec::new();
        for decl_pub in &rule_pub.declarations {
            decls.push(CoreDecl {
                name: decl_pub.name.clone(),
                value: decl_pub.value.clone(),
                important: decl_pub.important,
            });
        }
        let origin = match rule_pub.origin {
            CssOrigin::UserAgent => CoreOrigin::UserAgent,
            CssOrigin::User => CoreOrigin::User,
            CssOrigin::Author => CoreOrigin::Author,
        };
        rules_out.push(CoreRule {
            origin,
            source_order: rule_pub.source_order,
            prelude: rule_pub.prelude.clone(),
            declarations: decls,
        });
    }
    CoreSheet {
        rules: rules_out,
        origin: CoreOrigin::Author,
    }
}

/// Map a core RGBA color to the public RGBA struct.
/// Maps a core RGBA color to a public RGBA color.
const fn map_rgba(core_rgba: CoreRgba) -> Rgba {
    Rgba {
        red: core_rgba.red,
        green: core_rgba.green,
        blue: core_rgba.blue,
        alpha: core_rgba.alpha,
    }
}

/// Map core `Edges` to public `Edges` for margin/padding.
const fn map_edges(core_edges: CoreEdges) -> Edges {
    Edges {
        top: core_edges.top,
        right: core_edges.right,
        bottom: core_edges.bottom,
        left: core_edges.left,
    }
}

/// Map core `BorderWidths` to public `Edges` for border widths.
const fn map_border_widths_to_edges(core_border_widths: CoreBorderWidths) -> Edges {
    Edges {
        top: core_border_widths.top,
        right: core_border_widths.right,
        bottom: core_border_widths.bottom,
        left: core_border_widths.left,
    }
}

/// Map core border style enum to public border style.
const fn map_border_style(border_style: CoreBorderStyle) -> BorderStyle {
    match border_style {
        CoreBorderStyle::Solid => BorderStyle::Solid,
        CoreBorderStyle::None => BorderStyle::None,
    }
}

/// Map core display enum to public display for layout consumers.
const fn map_display(display: CoreDisplay) -> Display {
    match display {
        CoreDisplay::Flex => Display::Flex,
        CoreDisplay::Inline => Display::Inline,
        CoreDisplay::Block | CoreDisplay::Contents => Display::Block,
    }
}

/// Map core align-items enum to public align-items.
const fn map_align_items(align_items: CoreAlignItems) -> AlignItems {
    match align_items {
        CoreAlignItems::Center => AlignItems::Center,
        CoreAlignItems::FlexStart => AlignItems::FlexStart,
        CoreAlignItems::FlexEnd => AlignItems::FlexEnd,
        CoreAlignItems::Stretch => AlignItems::Stretch,
    }
}

/// Map core overflow enum to public overflow.
const fn map_overflow(overflow: CoreOverflow) -> Overflow {
    match overflow {
        CoreOverflow::Hidden => Overflow::Hidden,
        CoreOverflow::Visible => Overflow::Visible,
    }
}

/// Map core positioning enum to public positioning.
const fn map_position(position: CorePosition) -> Position {
    match position {
        CorePosition::Static => Position::Static,
        CorePosition::Relative => Position::Relative,
        CorePosition::Absolute => Position::Absolute,
        CorePosition::Fixed => Position::Fixed,
    }
}

/// Map a core computed style into the public `ComputedStyle` used by layouter/tests.
fn map_core_to_public(core_style: &CoreComputedStyle) -> ComputedStyle {
    ComputedStyle {
        color: map_rgba(core_style.color),
        background_color: map_rgba(core_style.background_color),
        border_width: map_border_widths_to_edges(core_style.border_width),
        border_style: map_border_style(core_style.border_style),
        border_color: map_rgba(core_style.border_color),
        display: map_display(core_style.display),
        margin: map_edges(core_style.margin),
        padding: map_edges(core_style.padding),
        flex_basis: core_style
            .flex_basis
            .map_or(LengthOrAuto::Auto, LengthOrAuto::Pixels),
        flex_grow: core_style.flex_grow,
        flex_shrink: core_style.flex_shrink,
        align_items: map_align_items(core_style.align_items),
        font_size: core_style.font_size,
        overflow: map_overflow(core_style.overflow),
        position: map_position(core_style.position),
        z_index: core_style.z_index,
        // Dimensions and extras: default until core exposes these; keep stable API
        box_sizing: BoxSizing::ContentBox,
        width: None,
        height: None,
        min_width: None,
        min_height: None,
        max_width: None,
        max_height: None,
        top: None,
        left: None,
        right: None,
        bottom: None,
        float: Float::None,
        clear: Clear::None,
    }
}
