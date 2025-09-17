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
    AlignItems as CoreAlignItems, BorderStyle as CoreBorderStyle,
    ComputedStyle as CoreComputedStyle, Display as CoreDisplay, Overflow as CoreOverflow,
    Position as CorePosition,
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
                    self.style_changed = true;
                    self.changed_nodes.push(node);
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

/// Map a core computed style into the public `ComputedStyle` used by layouter/tests.
fn map_core_to_public(core_style: &CoreComputedStyle) -> ComputedStyle {
    ComputedStyle {
        color: Rgba {
            red: core_style.color.red,
            green: core_style.color.green,
            blue: core_style.color.blue,
            alpha: core_style.color.alpha,
        },
        background_color: Rgba {
            red: core_style.background_color.red,
            green: core_style.background_color.green,
            blue: core_style.background_color.blue,
            alpha: core_style.background_color.alpha,
        },
        border_width: Edges {
            top: core_style.border_width.top,
            right: core_style.border_width.right,
            bottom: core_style.border_width.bottom,
            left: core_style.border_width.left,
        },
        border_style: match core_style.border_style {
            CoreBorderStyle::Solid => BorderStyle::Solid,
            CoreBorderStyle::None => BorderStyle::None,
        },
        border_color: Rgba {
            red: core_style.border_color.red,
            green: core_style.border_color.green,
            blue: core_style.border_color.blue,
            alpha: core_style.border_color.alpha,
        },
        display: match core_style.display {
            CoreDisplay::Flex => Display::Flex,
            CoreDisplay::Inline => Display::Inline,
            CoreDisplay::Block | CoreDisplay::Contents => Display::Block,
        },
        margin: Edges {
            top: core_style.margin.top,
            right: core_style.margin.right,
            bottom: core_style.margin.bottom,
            left: core_style.margin.left,
        },
        padding: Edges {
            top: core_style.padding.top,
            right: core_style.padding.right,
            bottom: core_style.padding.bottom,
            left: core_style.padding.left,
        },
        flex_basis: core_style
            .flex_basis
            .map_or(LengthOrAuto::Auto, LengthOrAuto::Pixels),
        flex_grow: core_style.flex_grow,
        flex_shrink: core_style.flex_shrink,
        align_items: match core_style.align_items {
            CoreAlignItems::Center => AlignItems::Center,
            CoreAlignItems::FlexStart => AlignItems::FlexStart,
            CoreAlignItems::FlexEnd => AlignItems::FlexEnd,
            CoreAlignItems::Stretch => AlignItems::Stretch,
        },
        font_size: core_style.font_size,
        overflow: match core_style.overflow {
            CoreOverflow::Hidden => Overflow::Hidden,
            CoreOverflow::Visible => Overflow::Visible,
        },
        position: match core_style.position {
            CorePosition::Static => Position::Static,
            CorePosition::Relative => Position::Relative,
            CorePosition::Absolute => Position::Absolute,
            CorePosition::Fixed => Position::Fixed,
        },
        z_index: core_style.z_index,
    }
}
