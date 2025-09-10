use std::collections::HashMap;
use js::{NodeKey, DOMSubscriber, DOMUpdate};
use crate::parser::StylesheetStreamParser;
use crate::types::{Origin, Stylesheet, StyleRule};
use url::Url;

pub mod parser;
pub mod rulemap;
pub mod selector;
pub mod types;
pub mod values;
pub mod ruledb;

/// A DOM mirror that discovers styles from the DOM stream.
/// - Aggregates inline <style> contents via a streaming CSS parser.
/// - Records discovered external stylesheets from <link rel="stylesheet" href="...">.
/// - Tracks rules per <style> node and retracts them when the node is removed.
pub struct CSSMirror {
    styles: Stylesheet,
    style_parsers: HashMap<NodeKey, StylesheetStreamParser>,
    /// Per-<style> node collected stylesheet (rules parsed for that node only).
    style_sheets: HashMap<NodeKey, Stylesheet>,
    link_nodes: HashMap<NodeKey, (Option<String>, Option<String>)>, // (rel, href)
    // Attributes may arrive before insertion; buffer them until we know the tag
    pending_link_attrs: HashMap<NodeKey, (Option<String>, Option<String>)>, // (rel, href)
    discovered_links: Vec<String>,
    next_order_base: u32,
    /// Base URL of the document used to resolve relative hrefs in <link>.
    base_url: Url,
}

impl CSSMirror {
    pub fn new() -> Self {
        Self {
            styles: Stylesheet::default(),
            style_parsers: HashMap::new(),
            style_sheets: HashMap::new(),
            link_nodes: HashMap::new(),
            pending_link_attrs: HashMap::new(),
            discovered_links: Vec::new(),
            next_order_base: 0,
            base_url: Url::parse("about:blank").unwrap(),
        }
    }

    /// Create a CSSMirror configured with a document base URL for resolving <link href>.
    pub fn with_base(base_url: Url) -> Self {
        Self {
            styles: Stylesheet::default(),
            style_parsers: HashMap::new(),
            style_sheets: HashMap::new(),
            link_nodes: HashMap::new(),
            pending_link_attrs: HashMap::new(),
            discovered_links: Vec::new(),
            next_order_base: 0,
            base_url,
        }
    }

    /// Combined stylesheet collected so far (inline only for now).
    pub fn styles(&self) -> &Stylesheet { &self.styles }

    /// Discovered external stylesheet hrefs (no fetching yet).
    pub fn discovered_stylesheets(&self) -> &[String] { &self.discovered_links }

    /// Return true if the rel attribute contains the token "stylesheet" (ASCII case-insensitive).
    fn rel_has_stylesheet_token(rel: &str) -> bool {
        rel.split_whitespace().any(|t| t.eq_ignore_ascii_case("stylesheet"))
    }

    /// Try to discover and record a stylesheet link given rel/href optionals.
    fn try_discover_stylesheet(&mut self, rel: &Option<String>, href: &Option<String>) {
        let Some(rel_str) = rel.as_ref() else { return; };
        if !Self::rel_has_stylesheet_token(rel_str) { return; }
        let Some(href_str) = href.as_ref() else { return; };
        // Resolve to absolute URL
        let resolved: Option<Url> = Url::parse(href_str).ok().or_else(|| self.base_url.join(href_str).ok());
        if let Some(abs) = resolved {
            let abs_s = abs.to_string();
            if !self.discovered_links.iter().any(|s| s == &abs_s) {
                self.discovered_links.push(abs_s);
            }
        }
    }

    /// Rebuild the aggregate stylesheet from all per-<style> node sheets.
    fn rebuild_aggregate(&mut self) {
        let mut all_rules: Vec<StyleRule> = Vec::new();
        for sheet in self.style_sheets.values() {
            all_rules.extend(sheet.rules.clone());
        }
        // Sort by source_order to stabilize cascade
        all_rules.sort_by_key(|r| r.source_order);
        self.styles.rules = all_rules;
    }
}

impl DOMSubscriber for CSSMirror {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), anyhow::Error> {
        use DOMUpdate::*;
        match update {
            InsertElement { parent: _, node, tag, pos: _ } => {
                if tag.eq_ignore_ascii_case("style") {
                    // Start a new stream parser for this <style> element and initialize its per-node sheet.
                    let parser = StylesheetStreamParser::new(Origin::Author, self.next_order_base);
                    self.style_parsers.insert(node, parser);
                    self.style_sheets.entry(node).or_insert_with(Stylesheet::default);
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
                    if let Some((rel, href)) = self.link_nodes.get(&node).cloned() {
                        self.try_discover_stylesheet(&rel, &href);
                    }
                }
            }
            InsertText { parent, node: _, text, pos: _ } => {
                // If text is a child of a <style> element, feed it into that node's sheet.
                if let Some(parser) = self.style_parsers.get_mut(&parent) {
                    let sheet = self.style_sheets.entry(parent).or_insert_with(Stylesheet::default);
                    parser.push_chunk(&text, sheet);
                    // Advance the global source-order counter based on the parser's position
                    self.next_order_base = self.next_order_base.max(parser.next_source_order());
                    // Rebuild aggregate to expose newly parsed complete rules immediately.
                    self.rebuild_aggregate();
                }
            }
            SetAttr { node, name, value } => {
                let lname = name.to_ascii_lowercase();
                // Record attributes destined for <link> nodes; attributes may arrive before insertion
                match lname.as_str() {
                    "rel" | "href" => {
                        if let Some((rel, href)) = self.link_nodes.get_mut(&node) {
                            if lname == "rel" { *rel = Some(value.clone()); }
                            else { *href = Some(value.clone()); }
                            // Check discovery condition now that we may have both (avoid borrow conflict by cloning)
                            let rel_clone = rel.clone();
                            let href_clone = href.clone();
                            self.try_discover_stylesheet(&rel_clone, &href_clone);
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
                // Retract any rules that came from this <style> node.
                self.style_sheets.remove(&node);
                self.rebuild_aggregate();
            }
            EndOfDocument => {
                // Finalize any remaining style parsers and append their rules to per-node sheets.
                let mut remaining = std::mem::take(&mut self.style_parsers);
                for (k, parser) in remaining.drain() {
                    let (extra, next) = parser.finish_with_next();
                    // Merge rules into the node's sheet preserving their source_order
                    let sheet = self.style_sheets.entry(k).or_insert_with(Stylesheet::default);
                    let mut extra_rules = extra.rules;
                    sheet.rules.append(&mut extra_rules);
                    // Advance the global counter to the parser's final position
                    if next > self.next_order_base { self.next_order_base = next; }
                }
                // Rebuild aggregate once all parsers are finalized.
                self.rebuild_aggregate();
            }
        }
        Ok(())
    }
}