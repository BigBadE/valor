use std::collections::HashMap;
use html::dom::NodeKey;
use html::dom::updating::{DOMSubscriber, DOMUpdate};
use crate::parser::{StylesheetStreamParser, parse_declarations};
use crate::types::{Origin, Stylesheet, Declaration};

pub mod parser;
pub mod rulemap;
pub mod selector;
pub mod types;
pub mod values;
pub mod ruledb;

/// A DOM mirror that discovers styles from the DOM stream.
/// - Aggregates inline <style> contents via a streaming CSS parser.
/// - Records discovered external stylesheets from <link rel="stylesheet" href="...">.
/// Note: Removing a <style> node after rules were added is not yet retracting
/// previously added rules; future work can switch to per-node sheets and lazy merge.
pub struct CSSMirror {
    styles: Stylesheet,
    style_parsers: HashMap<NodeKey, StylesheetStreamParser>,
    link_nodes: HashMap<NodeKey, (Option<String>, Option<String>)>, // (rel, href)
    // Attributes may arrive before insertion; buffer them until we know the tag
    pending_link_attrs: HashMap<NodeKey, (Option<String>, Option<String>)>, // (rel, href)
    discovered_links: Vec<String>,
    next_order_base: u32,
    // Inline style attribute declarations per element node
    inline_styles: HashMap<NodeKey, Vec<Declaration>>,
}

impl CSSMirror {
    pub fn new() -> Self {
        Self {
            styles: Stylesheet::default(),
            style_parsers: HashMap::new(),
            link_nodes: HashMap::new(),
            pending_link_attrs: HashMap::new(),
            discovered_links: Vec::new(),
            next_order_base: 0,
            inline_styles: HashMap::new(),
        }
    }

    /// Combined stylesheet collected so far (inline only for now).
    pub fn styles(&self) -> &Stylesheet { &self.styles }

    /// Discovered external stylesheet hrefs (no fetching yet).
    pub fn discovered_stylesheets(&self) -> &[String] { &self.discovered_links }

    /// Inline style attribute declarations for a given element node, if present.
    pub fn inline_declarations(&self, node: &NodeKey) -> Option<&[Declaration]> {
        self.inline_styles.get(node).map(|v| v.as_slice())
    }
}

impl DOMSubscriber for CSSMirror {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), anyhow::Error> {
        use DOMUpdate::*;
        match update {
            InsertElement { parent: _, node, tag, pos: _ } => {
                if tag.eq_ignore_ascii_case("style") {
                    // Start a new stream parser for this <style> element.
                    let parser = StylesheetStreamParser::new(Origin::Author, self.next_order_base);
                    // Bump order base by a large window to avoid overlaps across style blocks.
                    self.next_order_base = self.next_order_base.saturating_add(1_000_000);
                    self.style_parsers.insert(node, parser);
                } else if tag.eq_ignore_ascii_case("link") {
                    // Track attributes to detect rel=stylesheet + href later.
                    // Initialize tracked entry
                    let mut rel_href = (None::<String>, None::<String>);
                    // If attributes arrived before insertion, merge them now
                    if let Some((prel, phref)) = self.pending_link_attrs.remove(&node) {
                        if prel.is_some() { rel_href.0 = prel; }
                        if phref.is_some() { rel_href.1 = phref; }
                    }
                    self.link_nodes.insert(node, rel_href);
                    // If we now know it's a stylesheet link with href, emit discovery once.
                    if let Some((rel, href)) = self.link_nodes.get(&node) {
                        if rel.as_ref().map(|r| r.to_ascii_lowercase().contains("stylesheet")).unwrap_or(false) {
                            if let Some(h) = href.as_ref() {
                                if !self.discovered_links.iter().any(|e| e == h) {
                                    self.discovered_links.push(h.clone());
                                }
                            }
                        }
                    }
                }
            }
            InsertText { parent, node: _, text, pos: _ } => {
                // If text is a child of a <style> element, feed it.
                if let Some(parser) = self.style_parsers.get_mut(&parent) {
                    parser.push_chunk(&text, &mut self.styles);
                }
            }
            SetAttr { node, name, value } => {
                let lname = name.to_ascii_lowercase();
                // Handle inline style attribute
                if lname.as_str() == "style" {
                    let decls = parse_declarations(&value);
                    if decls.is_empty() {
                        self.inline_styles.remove(&node);
                    } else {
                        self.inline_styles.insert(node, decls);
                    }
                }

                // Record attributes destined for <link> nodes; attributes may arrive before insertion
                match lname.as_str() {
                    "rel" | "href" => {
                        if let Some((rel, href)) = self.link_nodes.get_mut(&node) {
                            if lname == "rel" { *rel = Some(value.clone()); }
                            else { *href = Some(value.clone()); }
                            // Check discovery condition now that we may have both
                            if rel.as_ref().map(|r| r.to_ascii_lowercase().contains("stylesheet")).unwrap_or(false) {
                                if let Some(h) = href.as_ref() {
                                    if !self.discovered_links.iter().any(|e| e == h) {
                                        self.discovered_links.push(h.clone());
                                    }
                                }
                            }
                        } else {
                            // Buffer until we know if this node is actually a <link>
                            let entry = self.pending_link_attrs.entry(node).or_insert((None, None));
                            if lname == "rel" { entry.0 = Some(value.clone()); } else { entry.1 = Some(value.clone()); }
                        }
                    }
                    _ => {}
                }
            }
            RemoveNode { node } => {
                // Drop any parser/trackers for this node.
                self.style_parsers.remove(&node);
                self.link_nodes.remove(&node);
                self.pending_link_attrs.remove(&node);
                // Drop any inline style declarations associated with this node.
                self.inline_styles.remove(&node);
                // Note: We do not retract rules that may have been parsed already.
            }
            EndOfDocument => {
                // Finalize any remaining style parsers and append their rules.
                let mut remaining = std::mem::take(&mut self.style_parsers);
                for (_k, parser) in remaining.drain() {
                    let extra = parser.finish();
                    // Merge rules preserving their source_order
                    self.styles.rules.extend(extra.rules);
                }
            }
        }
        Ok(())
    }
}