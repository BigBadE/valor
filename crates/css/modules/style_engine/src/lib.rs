//! Minimal CSS style engine facade used by tests and higher layers.
//!
//! This module now delegates style computation to the real css_core engine by
//! replaying the current layout snapshot (tags, structure) and attributes
//! (id/class/style) into `css_core::CoreEngine`. The resulting computed styles
//! are mapped into this crate's `ComputedStyle` for layouter and tests.

use anyhow::Result;
use core::mem::take;
use core::sync::atomic::Ordering;
use css::types::Stylesheet as CssStylesheet;
use css_core::CoreEngine as CoreCssEngine;
use css_core::types::{
    Declaration as CoreDecl, Origin as CoreOrigin, Rule as CoreRule, Stylesheet as CoreSheet,
};
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

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
    pub fn force_full_restyle(&mut self) {
        // Request a full restyle on the next recompute.
        self.style_changed = true;
    }

    #[inline]
    pub fn recompute_dirty(&mut self) {
        // Rebuild the core engine from the cached layout snapshot and attributes.
        self.core = CoreCssEngine::new();

        // Replay structure breadth-first from ROOT using cached element_children
        fn replay_node(
            core: &mut CoreCssEngine,
            tags_by_key: &HashMap<NodeKey, String>,
            attrs: &HashMap<NodeKey, HashMap<String, String>>,
            element_children: &HashMap<NodeKey, Vec<NodeKey>>,
            parent: NodeKey,
        ) {
            if let Some(children) = element_children.get(&parent) {
                for &child in children {
                    let tag = tags_by_key
                        .get(&child)
                        .cloned()
                        .unwrap_or_else(|| String::from("div"));
                    let _ = core.apply_dom_update(DOMUpdate::InsertElement {
                        parent,
                        node: child,
                        tag,
                        pos: 0,
                    });
                    if let Some(map) = attrs.get(&child) {
                        if let Some(id_val) = map.get("id") {
                            let _ = core.apply_dom_update(DOMUpdate::SetAttr {
                                node: child,
                                name: String::from("id"),
                                value: id_val.clone(),
                            });
                        }
                        if let Some(class_val) = map.get("class") {
                            let _ = core.apply_dom_update(DOMUpdate::SetAttr {
                                node: child,
                                name: String::from("class"),
                                value: class_val.clone(),
                            });
                        }
                        if let Some(style_val) = map.get("style") {
                            let _ = core.apply_dom_update(DOMUpdate::SetAttr {
                                node: child,
                                name: String::from("style"),
                                value: style_val.clone(),
                            });
                        }
                    }
                    // Recurse
                    replay_node(core, tags_by_key, attrs, element_children, child);
                }
            }
        }
        replay_node(
            &mut self.core,
            &self.tags_by_key,
            &self.attrs,
            &self.element_children,
            NodeKey::ROOT,
        );
        let _ = self.core.apply_dom_update(DOMUpdate::EndOfDocument);

        // Replace stylesheet in core
        self.core
            .replace_stylesheet(map_sheet_to_core(&self.stylesheet));

        // Compute styles in core and map back
        let _styles_changed = self.core.recompute_styles();
        let core_snapshot = self.core.computed_snapshot();
        let mut mapped: HashMap<NodeKey, ComputedStyle> = HashMap::new();
        for (key, cs) in core_snapshot {
            mapped.insert(key, map_core_to_public(&cs));
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
    pub fn take_and_clear_style_changed(&mut self) -> bool {
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
        use DOMUpdate::*;
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

fn map_sheet_to_core(sheet: &CssStylesheet) -> CoreSheet {
    let mut rules_out: Vec<CoreRule> = Vec::new();
    for r in &sheet.rules {
        let mut decls: Vec<CoreDecl> = Vec::new();
        for d in &r.declarations {
            decls.push(CoreDecl {
                name: d.name.clone(),
                value: d.value.clone(),
                important: d.important,
            });
        }
        let origin = match r.origin {
            css::types::Origin::UserAgent => CoreOrigin::UserAgent,
            css::types::Origin::User => CoreOrigin::User,
            css::types::Origin::Author => CoreOrigin::Author,
        };
        rules_out.push(CoreRule {
            origin,
            source_order: r.source_order,
            prelude: r.prelude.clone(),
            declarations: decls,
        });
    }
    CoreSheet {
        rules: rules_out,
        origin: CoreOrigin::Author,
    }
}

fn map_core_to_public(cs: &css_core::style_model::ComputedStyle) -> ComputedStyle {
    // Map fields we serialize in the layout comparer harness
    ComputedStyle {
        color: Rgba {
            red: cs.color.red,
            green: cs.color.green,
            blue: cs.color.blue,
            alpha: cs.color.alpha,
        },
        background_color: Rgba {
            red: cs.background_color.red,
            green: cs.background_color.green,
            blue: cs.background_color.blue,
            alpha: cs.background_color.alpha,
        },
        border_width: Edges {
            top: cs.border_width.top,
            right: cs.border_width.right,
            bottom: cs.border_width.bottom,
            left: cs.border_width.left,
        },
        border_style: match cs.border_style {
            css_core::style_model::BorderStyle::Solid => BorderStyle::Solid,
            _ => BorderStyle::None,
        },
        border_color: Rgba {
            red: cs.border_color.red,
            green: cs.border_color.green,
            blue: cs.border_color.blue,
            alpha: cs.border_color.alpha,
        },
        display: match cs.display {
            css_core::style_model::Display::Flex => Display::Flex,
            css_core::style_model::Display::Inline => Display::Inline,
            css_core::style_model::Display::InlineFlex => Display::InlineFlex,
            _ => Display::Block,
        },
        margin: Edges {
            top: cs.margin.top,
            right: cs.margin.right,
            bottom: cs.margin.bottom,
            left: cs.margin.left,
        },
        padding: Edges {
            top: cs.padding.top,
            right: cs.padding.right,
            bottom: cs.padding.bottom,
            left: cs.padding.left,
        },
        flex_basis: match cs.flex_basis {
            Some(px) => LengthOrAuto::Pixels(px),
            None => LengthOrAuto::Auto,
        },
        flex_grow: cs.flex_grow,
        flex_shrink: cs.flex_shrink,
        align_items: match cs.align_items {
            css_core::style_model::AlignItems::Center => AlignItems::Center,
            css_core::style_model::AlignItems::FlexStart => AlignItems::FlexStart,
            css_core::style_model::AlignItems::FlexEnd => AlignItems::FlexEnd,
            _ => AlignItems::Stretch,
        },
        font_size: cs.font_size,
        overflow: match cs.overflow {
            css_core::style_model::Overflow::Hidden => Overflow::Hidden,
            _ => Overflow::Visible,
        },
        position: match cs.position {
            css_core::style_model::Position::Relative => Position::Relative,
            css_core::style_model::Position::Absolute => Position::Absolute,
            css_core::style_model::Position::Fixed => Position::Fixed,
            _ => Position::Static,
        },
        z_index: cs.z_index,
    }
}
