//! CSS orchestrator and mirror types exposed to other crates.
use anyhow::Result;
use js::DOMUpdate::{EndOfDocument, InsertElement, InsertText, RemoveNode, SetAttr};
use js::{DOMSubscriber, DOMUpdate};
use std::collections::HashMap;
use url::Url;

pub use js::NodeKey;

// Bring core types into scope to avoid fully qualified paths and satisfy clippy
use crate::parser::parse_stylesheet;
use css_orchestrator::CoreEngine;

pub mod style_types;
use crate::style_types::ComputedStyle;

pub mod types;

pub mod parser;

pub mod layout_helpers;

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
}

impl DOMSubscriber for Orchestrator {
    #[inline]
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        self.apply_dom_update(update)
    }
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
                    // Rebuild aggregated stylesheet when a new <style> is inserted.
                    self.rebuild_styles_from_style_nodes();
                }
            }
            InsertText { parent, text, .. } => {
                if self.style_text_by_node.contains_key(&parent) {
                    let entry = self.style_text_by_node.entry(parent).or_default();
                    entry.push_str(&text);
                    // Rebuild aggregated stylesheet when <style> text changes.
                    self.rebuild_styles_from_style_nodes();
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
            SetAttr { .. } => {}
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
}
impl Default for Orchestrator {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Orchestrator {
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
        // The public types are re-exports of core types, so we can pass through directly.
        self.core.replace_stylesheet(sheet.clone());
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
        let computed_styles = self.core.computed_snapshot();

        Ok(ProcessArtifacts {
            styles_changed,
            computed_styles,
        })
    }
}
