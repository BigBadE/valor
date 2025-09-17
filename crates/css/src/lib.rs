//! CSS orchestrator and mirror types exposed to other crates.
use anyhow::Result;
use js::DOMUpdate::{EndOfDocument, InsertElement, InsertText, RemoveNode, SetAttr};
use js::{DOMSubscriber, DOMUpdate};
use std::collections::HashMap;
use url::Url;

pub use js::NodeKey;

// Bring core types into scope to avoid fully qualified paths and satisfy clippy
use crate::parser::parse_stylesheet;
use css_core::CoreEngine;
use css_core::layout_model::{LayoutNodeKind as CoreLayoutNodeKind, LayoutRect as CoreLayoutRect};
use css_core::types::{
    Declaration as CoreDeclaration, Origin as CoreOrigin, Rule as CoreRule,
    Stylesheet as CoreStylesheet,
};

pub mod style_types;
use crate::style_types::{ComputedStyle, LayoutNodeKind, LayoutRect};
use std::collections::HashSet;

pub mod types;

pub mod parser;

pub mod layout_helpers;

/// Snapshot of layout nodes and their child ordering for inspection.
type LayoutSnapshot = Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)>;
/// Snapshot type from the core layout engine used for mapping into public structures.
type CoreLayoutSnapshot = Vec<(NodeKey, CoreLayoutNodeKind, Vec<NodeKey>)>;

pub struct CSSMirror {
    /// Base URL used for resolving discovered stylesheet links.
    _base: Option<Url>,
    /// Aggregated parsed stylesheet from in-document <style> nodes.
    styles: types::Stylesheet,
    /// Absolute URLs of discovered external stylesheets.
    discovered: Vec<String>,
    /// Track discovered <style> nodes and their text content in insertion order
    style_nodes_order: Vec<NodeKey>,
    /// Map from style node key to its accumulated text content.
    style_text_by_node: HashMap<NodeKey, String>,
    /// Style nodes to be skipped (e.g., test-injected reset styles).
    style_skip_nodes: HashSet<NodeKey>,
}

impl Default for CSSMirror {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl CSSMirror {
    #[inline]
    pub fn new() -> Self {
        // Avoid unwrap by deferring base initialization.
        Self {
            _base: None,
            styles: types::Stylesheet::default(),
            discovered: Vec::new(),
            style_nodes_order: Vec::new(),
            style_text_by_node: HashMap::new(),
            style_skip_nodes: HashSet::new(),
        }
    }
    #[inline]
    pub fn with_base(url: Url) -> Self {
        Self {
            _base: Some(url),
            styles: types::Stylesheet::default(),
            discovered: Vec::new(),
            style_nodes_order: Vec::new(),
            style_text_by_node: HashMap::new(),
            style_skip_nodes: HashSet::new(),
        }
    }
    /// Mutable reference to the aggregated in-document stylesheet.
    #[inline]
    pub const fn styles(&mut self) -> &mut types::Stylesheet {
        &mut self.styles
    }

    #[inline]
    pub fn discovered_stylesheets(&self) -> Vec<String> {
        self.discovered.clone()
    }

    /// Rebuild the aggregated stylesheet from tracked <style> nodes in DOM order.
    fn rebuild_styles_from_style_nodes(&mut self) {
        let mut out = types::Stylesheet::default();
        let mut base: u32 = 0;
        for node in &self.style_nodes_order {
            if self.style_skip_nodes.contains(node) {
                continue;
            }
            if let Some(text) = self.style_text_by_node.get(node) {
                let parsed = parser::parse_stylesheet(text, out.origin, base);
                // Avoid truncation on 64-bit by saturating len to u32::MAX
                let addend = u32::try_from(parsed.rules.len()).map_or(u32::MAX, |count| count);
                base = base.saturating_add(addend);
                out.rules.extend(parsed.rules);
            }
        }
        self.styles = out;
    }

    /// Apply the aggregated in-document stylesheet to the given orchestrator.
    /// This is the minimal glue to feed parsed CSS into the core engine.
    #[inline]
    pub fn apply_to_orchestrator(&self, orchestrator: &mut Orchestrator) {
        orchestrator.replace_stylesheet(&self.styles);
    }
}

impl DOMSubscriber for CSSMirror {
    #[inline]
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        match update {
            InsertElement {
                parent: _parent,
                node,
                tag,
                ..
            } => {
                if tag.eq_ignore_ascii_case("style") && !self.style_text_by_node.contains_key(&node)
                {
                    self.style_nodes_order.push(node);
                    self.style_text_by_node.insert(node, String::new());
                }
            }
            InsertText { parent, text, .. } => {
                if self.style_text_by_node.contains_key(&parent) {
                    let entry = self.style_text_by_node.entry(parent).or_default();
                    entry.push_str(&text);
                }
            }
            RemoveNode { node } => {
                if self.style_text_by_node.remove(&node).is_some() {
                    self.style_nodes_order.retain(|node_id| *node_id != node);
                    // Retract rules for this style node immediately
                    self.rebuild_styles_from_style_nodes();
                }
            }
            EndOfDocument => {
                self.rebuild_styles_from_style_nodes();
            }
            SetAttr { node, name, value } => {
                // If this is a test-injected reset style element, mark it to be skipped.
                if name.eq_ignore_ascii_case("data-valor-test-reset")
                    && value == "1"
                    && self.style_text_by_node.contains_key(&node)
                {
                    self.style_skip_nodes.insert(node);
                    self.rebuild_styles_from_style_nodes();
                }
            }
        }
        Ok(())
    }
}

pub struct Orchestrator {
    /// Core CSS engine that performs style and layout computation.
    core: CoreEngine,
}

pub struct ProcessArtifacts {
    pub styles_changed: bool,
    pub computed_styles: HashMap<NodeKey, ComputedStyle>,
    pub layout_snapshot: LayoutSnapshot,
    pub rects: HashMap<NodeKey, LayoutRect>,
    pub dirty_rects: Vec<LayoutRect>,
}
impl Default for Orchestrator {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Orchestrator {
    #[inline]
    /// Map core layout rects to public `LayoutRect` deterministically.
    fn map_rects(core_rects: HashMap<NodeKey, CoreLayoutRect>) -> HashMap<NodeKey, LayoutRect> {
        let mut out: HashMap<NodeKey, LayoutRect> = HashMap::new();
        let mut pairs: Vec<(NodeKey, CoreLayoutRect)> = core_rects.into_iter().collect();
        pairs.sort_by_key(|&(key, _)| key.0);
        for (key, rect) in pairs {
            out.insert(
                key,
                LayoutRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                },
            );
        }
        out
    }

    #[inline]
    /// Map core layout snapshot to public snapshot types.
    fn map_layout_snapshot(core_snapshot: CoreLayoutSnapshot) -> LayoutSnapshot {
        core_snapshot
            .into_iter()
            .map(|(key, kind, children)| {
                let mapped_kind = match kind {
                    CoreLayoutNodeKind::Document => LayoutNodeKind::Document,
                    CoreLayoutNodeKind::Block { tag } => LayoutNodeKind::Block { tag },
                    CoreLayoutNodeKind::InlineText { text } => LayoutNodeKind::InlineText { text },
                };
                (key, mapped_kind, children)
            })
            .collect()
    }

    #[inline]
    pub fn new() -> Self {
        Self {
            core: CoreEngine::new(),
        }
    }

    /// Apply a `DOMUpdate` to the core engine.
    ///
    /// # Errors
    /// Returns an error if the core engine reports a failure during update application.
    #[inline]
    pub fn apply_dom_update(&mut self, update: DOMUpdate) -> Result<()> {
        self.core.apply_dom_update(update)
    }

    /// Replace the current stylesheet used by the engine.
    #[inline]
    pub fn replace_stylesheet(&mut self, sheet: &types::Stylesheet) {
        // Map orchestrator public type to core type, including rules
        let mut core_rules = Vec::new();
        for rule_pub in &sheet.rules {
            let mut core_decls = Vec::new();
            for decl_pub in &rule_pub.declarations {
                core_decls.push(CoreDeclaration {
                    name: decl_pub.name.clone(),
                    value: decl_pub.value.clone(),
                    important: decl_pub.important,
                });
            }
            core_rules.push(CoreRule {
                origin: match rule_pub.origin {
                    types::Origin::UserAgent => CoreOrigin::UserAgent,
                    types::Origin::User => CoreOrigin::User,
                    types::Origin::Author => CoreOrigin::Author,
                },
                source_order: rule_pub.source_order,
                prelude: rule_pub.prelude.clone(),
                declarations: core_decls,
            });
        }
        let core_sheet = CoreStylesheet {
            rules: core_rules,
            origin: match sheet.origin {
                types::Origin::UserAgent => CoreOrigin::UserAgent,
                types::Origin::User => CoreOrigin::User,
                types::Origin::Author => CoreOrigin::Author,
            },
        };
        self.core.replace_stylesheet(core_sheet);
    }

    /// Parse the provided CSS text with the given origin and replace the current stylesheet.
    /// This allows callers to bypass `CSSMirror` and feed raw CSS into the engine.
    #[inline]
    pub fn replace_stylesheet_from_css(&mut self, css_text: &str, origin: types::Origin) {
        let parsed = parse_stylesheet(css_text, origin, 0);
        self.replace_stylesheet(&parsed);
    }
    /// Execute one processing pass and return artifacts for rendering and inspection.
    ///
    /// # Errors
    /// Returns an error if the core engine encounters a failure during processing.
    #[inline]
    pub fn process_once(&mut self) -> Result<ProcessArtifacts> {
        let styles_changed = self.core.recompute_styles();
        let core_computed = self.core.computed_snapshot();
        let core_rects = self.core.compute_layout();
        let core_dirty = self.core.take_dirty_rects();
        let core_snapshot = self.core.layout_snapshot();

        // Map core types to public orchestrator types
        let computed = core_computed;

        let rects = Self::map_rects(core_rects);

        let dirty_rects: Vec<LayoutRect> = core_dirty
            .into_iter()
            .map(|rect| LayoutRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            })
            .collect();

        let layout_snapshot: LayoutSnapshot = Self::map_layout_snapshot(core_snapshot);
        Ok(ProcessArtifacts {
            styles_changed,
            computed_styles: computed,
            layout_snapshot,
            rects,
            dirty_rects,
        })
    }
}
