//! CSS orchestrator and mirror types exposed to other crates.
use anyhow::Result;
use js::DOMUpdate::{EndOfDocument, InsertElement, InsertText, RemoveNode};
use js::{DOMSubscriber, DOMUpdate};
use std::collections::HashMap;
use url::Url;

pub use js::NodeKey;

// Re-export StyleDatabase from css_orchestrator
pub use css_orchestrator::StyleDatabase;

pub mod style_types;

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
    /// Debug counter for rebuild tracking
    rebuild_counter: u32,
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
            rebuild_counter: 0,
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
            rebuild_counter: 0,
        }
    }
    /// Mutable reference to the aggregated in-document stylesheet.
    #[inline]
    pub fn styles(&mut self) -> &mut types::Stylesheet {
        &mut self.styles
    }

    #[inline]
    pub fn discovered_stylesheets(&self) -> Vec<String> {
        self.discovered.clone()
    }

    /// Rebuild the aggregated stylesheet from tracked <style> nodes in DOM order.
    fn rebuild_styles_from_style_nodes(&mut self) {
        self.rebuild_counter += 1;

        let mut out = types::Stylesheet::default();
        let mut base: u32 = 0;

        // Sort nodes so injected nodes (with high NodeKey IDs) come LAST
        // This ensures CSS reset injected via inject_css_sync comes after HTML styles
        let mut sorted_nodes = self.style_nodes_order.clone();
        sorted_nodes.sort_by_key(|node_key| {
            // Injected nodes have IDs starting at 0xFFFF_0000
            // Normal HTML nodes have lower IDs
            // This sort puts HTML nodes first, injected nodes last
            node_key.0
        });

        for node in &sorted_nodes {
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
            DOMUpdate::UpdateText { .. } | DOMUpdate::SetAttr { .. } => {
                // UpdateText doesn't affect CSS mirror since it only updates text nodes,
                // and CSS is only collected from <style> element children via InsertText
                // SetAttr also doesn't affect CSS mirror
            }
            EndOfDocument => {
                self.rebuild_styles_from_style_nodes();
            }
        }
        Ok(())
    }
}
