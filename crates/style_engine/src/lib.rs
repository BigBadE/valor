use anyhow::Error;
use css::parser::{StylesheetStreamParser, parse_declarations};
use css::types::{Origin, Stylesheet};
use css::rulemap::{RuleMap, RuleRef, index_rules};
use css::selector::{ComplexSelector, CompoundSelector, SimpleSelector, Combinator};
use css::ruledb::RuleDB;
use html::dom::NodeKey;
use html::dom::updating::{DOMSubscriber, DOMUpdate};
use log::{info, trace};
use std::collections::HashMap;
use std::collections::HashSet;

mod computed_style;
pub use computed_style::{ComputedStyle, Display, Edges, ColorRGBA, SizeSpecified};

/// Internal node info tracked by the StyleEngine mirror for minimal style computation.
#[derive(Debug, Clone)]
struct NodeInfo {
    tag: String,
    id: Option<String>,
    classes: HashSet<String>,
    parent: Option<NodeKey>,
    children: Vec<NodeKey>,
    inline_display: Option<Display>,
    inline_width: Option<SizeSpecified>,
    inline_height: Option<SizeSpecified>,
    inline_margin: Option<Edges>,
    inline_padding: Option<Edges>,
}

pub type ComputedMap = HashMap<NodeKey, ComputedStyle>;

/// StyleEngine is a DOM subscriber that will own selector matching,
/// cascade and computed style generation in future steps.
/// For now it installs a minimal UA stylesheet, accepts Author Stylesheet updates,
/// merges them, and mirrors DOM updates while computing a very small subset:
/// - display defaults by tag (UA)
/// - inline style attribute display override (Author)
pub struct StyleEngine {
    ua_stylesheet: Stylesheet,
    author_stylesheet: Stylesheet,
    stylesheet: Stylesheet,
    ruledb: RuleDB,
    nodes: HashMap<NodeKey, NodeInfo>,
    computed: ComputedMap,
    rule_index: RuleMap,
    matches: HashMap<NodeKey, Vec<RuleRef>>,
    nodes_by_id: HashMap<String, Vec<NodeKey>>,
    nodes_by_class: HashMap<String, Vec<NodeKey>>,
    nodes_by_tag: HashMap<String, Vec<NodeKey>>,
}

impl StyleEngine {
    pub fn new() -> Self {
        let ua_stylesheet = build_ua_stylesheet();
        let stylesheet = ua_stylesheet.clone();
        let ruledb = RuleDB::from_stylesheet(&stylesheet);
        Self {
            ua_stylesheet,
            author_stylesheet: Stylesheet::default(),
            stylesheet,
            ruledb,
            nodes: HashMap::new(),
            computed: HashMap::new(),
            rule_index: RuleMap::new(),
            matches: HashMap::new(),
            nodes_by_id: HashMap::new(),
            nodes_by_class: HashMap::new(),
            nodes_by_tag: HashMap::new(),
        }
    }

    /// Replace the active author stylesheet set with a new snapshot and merge with UA sheet.
    pub fn replace_stylesheet(&mut self, author: Stylesheet) {
        self.author_stylesheet = author;
        self.stylesheet = merge_stylesheets(&self.ua_stylesheet, &self.author_stylesheet);
        // Rebuild RuleDB for the merged stylesheet
        self.ruledb = RuleDB::from_stylesheet(&self.stylesheet);
        // Rebuild rule index and recompute matches for all nodes (Phase 2 skeleton)
        self.rebuild_rule_index();
        self.rematch_all_nodes();
        info!(
            "StyleEngine: merged UA+Author stylesheets (ua_rules={}, author_rules={}, indexed_rules={})",
            self.ua_stylesheet.rules.len(),
            self.author_stylesheet.rules.len(),
            self.stylesheet.rules.len()
        );
    }

    /// Read-only access to the current merged stylesheet snapshot.
    pub fn stylesheet(&self) -> &Stylesheet { &self.stylesheet }

    /// Return a cloned snapshot of computed styles per node (minimal subset for now).
    pub fn computed_snapshot(&self) -> ComputedMap { self.computed.clone() }

    fn compute_for_info(info: &NodeInfo) -> ComputedStyle {
        let mut cs = ComputedStyle::default();
        // UA default
        cs.display = default_display_for_tag(&info.tag);
        // Inline style overrides (Author)
        if let Some(d) = info.inline_display { cs.display = d; }
        if let Some(w) = info.inline_width { cs.width = w; }
        if let Some(h) = info.inline_height { cs.height = h; }
        if let Some(m) = info.inline_margin { cs.margin = m; }
        if let Some(p) = info.inline_padding { cs.padding = p; }
        cs
    }

    fn rebuild_rule_index(&mut self) {
        let mut map = RuleMap::new();
        index_rules(&self.stylesheet, &mut map);
        self.rule_index = map;
    }

    fn rematch_all_nodes(&mut self) {
        let keys: Vec<NodeKey> = self.nodes.keys().cloned().collect();
        for k in keys { self.rematch_node(k); }
    }

    fn rematch_node(&mut self, node: NodeKey) {
        // Build candidates from rule_index using id, classes, tag and universal
        let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
        let mut cands: Vec<RuleRef> = Vec::new();
        if let Some(info) = self.nodes.get(&node) {
            if let Some(id) = info.id.as_ref() {
                if let Some(v) = self.rule_index.by_id.get(id) { for rr in v { if seen.insert((rr.rule_idx, rr.selector_idx)) { cands.push(*rr); } } }
            }
            for class in &info.classes {
                if let Some(v) = self.rule_index.by_class.get(class) { for rr in v { if seen.insert((rr.rule_idx, rr.selector_idx)) { cands.push(*rr); } } }
            }
            let tag_lc = info.tag.to_ascii_lowercase();
            if let Some(v) = self.rule_index.by_tag.get(&tag_lc) { for rr in v { if seen.insert((rr.rule_idx, rr.selector_idx)) { cands.push(*rr); } } }
            for rr in &self.rule_index.universal { if seen.insert((rr.rule_idx, rr.selector_idx)) { cands.push(*rr); } }
        }
        // Filter by full selector match
        let mut matched: Vec<RuleRef> = Vec::new();
        for rr in cands {
            if let Some(rule) = self.stylesheet.rules.get(rr.rule_idx) {
                if let Some(sel) = rule.selectors.get(rr.selector_idx) {
                    if self.match_complex_selector(node, sel) { matched.push(rr); }
                }
            }
        }
        self.matches.insert(node, matched);
    }

    fn match_complex_selector(&self, node: NodeKey, sel: &ComplexSelector) -> bool {
        if sel.sequence.is_empty() { return false; }
        // Start from rightmost compound
        let mut current = node;
        let mut idx: isize = sel.sequence.len() as isize - 1;
        // Ensure rightmost compound matches the node
        let (last_comp, _) = &sel.sequence[idx as usize];
        if !self.match_compound(current, last_comp) { return false; }
        while idx > 0 {
            let (comp, comb_opt) = &sel.sequence[(idx - 1) as usize];
            let comb = comb_opt.unwrap_or(Combinator::Descendant);
            match comb {
                Combinator::Descendant => {
                    // climb ancestors to find a match
                    let mut p = self.nodes.get(&current).and_then(|ni| ni.parent);
                    let mut found = false;
                    while let Some(anc) = p {
                        if self.match_compound(anc, comp) { current = anc; found = true; break; }
                        p = self.nodes.get(&anc).and_then(|ni| ni.parent);
                    }
                    if !found { return false; }
                }
                Combinator::Child => {
                    let p = self.nodes.get(&current).and_then(|ni| ni.parent);
                    if let Some(anc) = p { if self.match_compound(anc, comp) { current = anc; } else { return false; } }
                    else { return false; }
                }
                Combinator::NextSibling | Combinator::SubsequentSibling => {
                    // Not supported in Phase 2 skeleton
                    return false;
                }
            }
            idx -= 1;
        }
        true
    }

    fn match_compound(&self, node: NodeKey, comp: &CompoundSelector) -> bool {
        let Some(info) = self.nodes.get(&node) else { return false; };
        for s in &comp.simples {
            match s {
                SimpleSelector::Universal => {}
                SimpleSelector::Type(t) => {
                    if info.tag.eq_ignore_ascii_case(t) == false { return false; }
                }
                SimpleSelector::Id(id) => {
                    if info.id.as_ref().map(|v| v == id).unwrap_or(false) == false { return false; }
                }
                SimpleSelector::Class(c) => {
                    if !info.classes.contains(c) { return false; }
                }
            }
        }
        true
    }

    fn update_id_index(&mut self, node: NodeKey, old_id: Option<String>, new_id: Option<String>) {
        if let Some(oid) = old_id { if let Some(v) = self.nodes_by_id.get_mut(&oid) { v.retain(|k| *k != node); } }
        if let Some(nid) = new_id { self.nodes_by_id.entry(nid).or_default().push(node); }
    }

    fn update_class_index(&mut self, node: NodeKey, old: &HashSet<String>, new: &HashSet<String>) {
        for c in old { if !new.contains(c) { if let Some(v) = self.nodes_by_class.get_mut(c) { v.retain(|k| *k != node); } } }
        for c in new { if !old.contains(c) { self.nodes_by_class.entry(c.clone()).or_default().push(node); } }
    }

    fn add_tag_index(&mut self, node: NodeKey, tag: &str) {
        self.nodes_by_tag.entry(tag.to_ascii_lowercase()).or_default().push(node);
    }

    fn remove_node_recursive(&mut self, node: NodeKey) {
        if let Some(info) = self.nodes.remove(&node) {
            // remove from parent children
            if let Some(p) = info.parent { if let Some(pi) = self.nodes.get_mut(&p) { pi.children.retain(|k| *k != node); } }
            // drop indexes
            if let Some(idv) = info.id { if let Some(v) = self.nodes_by_id.get_mut(&idv) { v.retain(|k| *k != node); } }
            for c in info.classes { if let Some(v) = self.nodes_by_class.get_mut(&c) { v.retain(|k| *k != node); } }
            // tag index
            if let Some(v) = self.nodes_by_tag.get_mut(&info.tag.to_ascii_lowercase()) { v.retain(|k| *k != node); }
            self.matches.remove(&node);
            self.computed.remove(&node);
            // recurse
            for ch in info.children { self.remove_node_recursive(ch); }
        }
    }
}

impl DOMSubscriber for StyleEngine {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        use DOMUpdate::*;
        match update {
            InsertElement { parent, node, tag, pos: _ } => {
                // Merge with any pending inline info that may have arrived via SetAttr before InsertElement
                let pending = self.nodes.get(&node).cloned();
                let info = NodeInfo {
                    tag: tag.clone(),
                    id: pending.as_ref().and_then(|p| p.id.clone()),
                    classes: pending.as_ref().map(|p| p.classes.clone()).unwrap_or_default(),
                    parent: Some(parent),
                    children: pending.as_ref().map(|p| p.children.clone()).unwrap_or_default(),
                    inline_display: pending.as_ref().and_then(|p| p.inline_display),
                    inline_width: pending.as_ref().and_then(|p| p.inline_width),
                    inline_height: pending.as_ref().and_then(|p| p.inline_height),
                    inline_margin: pending.as_ref().and_then(|p| p.inline_margin),
                    inline_padding: pending.as_ref().and_then(|p| p.inline_padding),
                };
                let cs = StyleEngine::compute_for_info(&info);
                self.nodes.insert(node, info);
                // link parent→child
                if let Some(pinfo) = self.nodes.get_mut(&parent) {
                    if !pinfo.children.contains(&node) { pinfo.children.push(node); }
                } else {
                    self.nodes.insert(parent, NodeInfo {
                        tag: String::new(), id: None, classes: HashSet::new(), parent: None, children: vec![node],
                        inline_display: None, inline_width: None, inline_height: None, inline_margin: None, inline_padding: None,
                    });
                }
                self.computed.insert(node, cs);
                self.add_tag_index(node, &tag);
                // Phase 2: recompute selector matches for this node
                self.rematch_node(node);
            }
            InsertText { .. } => {
                // No computed style for text nodes at the moment.
            }
            SetAttr { node, name, value } => {
                // Track inline style overrides (display, width, height, margin/padding)
                if name.eq_ignore_ascii_case("style") {
                    let parsed = parse_declarations(&value);
                    // Start from existing or new placeholder info
                    let mut info = if let Some(existing) = self.nodes.get(&node).cloned() {
                        existing
                    } else {
                        NodeInfo { tag: String::new(), id: None, classes: HashSet::new(), parent: None, children: Vec::new(), inline_display: None, inline_width: None, inline_height: None, inline_margin: None, inline_padding: None }
                    };
                    let mut inline_display: Option<Display> = info.inline_display;
                    let mut inline_width: Option<SizeSpecified> = info.inline_width;
                    let mut inline_height: Option<SizeSpecified> = info.inline_height;
                    let mut margin: Edges = info.inline_margin.unwrap_or_default();
                    let mut have_margin = info.inline_margin.is_some();
                    let mut padding: Edges = info.inline_padding.unwrap_or_default();
                    let mut have_padding = info.inline_padding.is_some();
                    for d in parsed {
                        let prop = d.name.to_ascii_lowercase();
                        let val = d.value.trim();
                        match prop.as_str() {
                            "display" => {
                                let v = val.to_ascii_lowercase();
                                inline_display = match v.as_str() {
                                    "none" => Some(Display::None),
                                    "block" => Some(Display::Block),
                                    "inline" => Some(Display::Inline),
                                    _ => inline_display,
                                };
                            }
                            "width" => {
                                let v = val.to_ascii_lowercase();
                                inline_width = parse_size_spec(&v).or(inline_width);
                            }
                            "height" => {
                                let v = val.to_ascii_lowercase();
                                inline_height = parse_size_spec(&v).or(inline_height);
                            }
                            "margin" => {
                                if let Some(e) = parse_edges_shorthand(val) {
                                    margin = e; have_margin = true;
                                }
                            }
                            "padding" => {
                                if let Some(e) = parse_edges_shorthand(val) {
                                    padding = e; have_padding = true;
                                }
                            }
                            "margin-top" => { if let Some(px) = parse_px(val) { margin.top = px; have_margin = true; } }
                            "margin-right" => { if let Some(px) = parse_px(val) { margin.right = px; have_margin = true; } }
                            "margin-bottom" => { if let Some(px) = parse_px(val) { margin.bottom = px; have_margin = true; } }
                            "margin-left" => { if let Some(px) = parse_px(val) { margin.left = px; have_margin = true; } }
                            "padding-top" => { if let Some(px) = parse_px(val) { padding.top = px; have_padding = true; } }
                            "padding-right" => { if let Some(px) = parse_px(val) { padding.right = px; have_padding = true; } }
                            "padding-bottom" => { if let Some(px) = parse_px(val) { padding.bottom = px; have_padding = true; } }
                            "padding-left" => { if let Some(px) = parse_px(val) { padding.left = px; have_padding = true; } }
                            _ => {}
                        }
                    }
                    info.inline_display = inline_display;
                    info.inline_width = inline_width;
                    info.inline_height = inline_height;
                    if have_margin { info.inline_margin = Some(margin); }
                    if have_padding { info.inline_padding = Some(padding); }
                    // Store back and compute (may be overwritten on InsertElement when tag is known)
                    let cs = StyleEngine::compute_for_info(&info);
                    self.nodes.insert(node, info);
                    self.computed.insert(node, cs);
                } else if name.eq_ignore_ascii_case("id") {
                    let mut info = if let Some(existing) = self.nodes.get(&node).cloned() { existing } else { NodeInfo { tag: String::new(), id: None, classes: HashSet::new(), parent: None, children: Vec::new(), inline_display: None, inline_width: None, inline_height: None, inline_margin: None, inline_padding: None } };
                    let old = info.id.clone();
                    let new_id = if value.is_empty() { None } else { Some(value.clone()) };
                    info.id = new_id.clone();
                    self.nodes.insert(node, info);
                    self.update_id_index(node, old, new_id);
                    self.rematch_node(node);
                } else if name.eq_ignore_ascii_case("class") {
                    let mut info = if let Some(existing) = self.nodes.get(&node).cloned() { existing } else { NodeInfo { tag: String::new(), id: None, classes: HashSet::new(), parent: None, children: Vec::new(), inline_display: None, inline_width: None, inline_height: None, inline_margin: None, inline_padding: None } };
                    let old = info.classes.clone();
                    let new: HashSet<String> = value.split_whitespace().filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
                    info.classes = new.clone();
                    self.nodes.insert(node, info);
                    self.update_class_index(node, &old, &new);
                    self.rematch_node(node);
                }
            }
            RemoveNode { node } => {
                self.remove_node_recursive(node);
            }
            EndOfDocument => {
                // No-op for now; future work: finalize and broadcast updates
            }
        }
        Ok(())
    }
}

/// Build a minimal UA stylesheet with standard display defaults and basic body margin.
fn build_ua_stylesheet() -> Stylesheet {
    // Keep this small; more tags can be added as layout grows.
    const UA_CSS: &str = r#"
html { display: block }
body { display: block; margin: 8px }
div, p, header, main, footer, section, article, nav, ul, ol, li, h1, h2, h3, h4, h5, h6 { display: block }
span, a, b, i, strong, em { display: inline }
style, script { display: none }
"#;
    let mut ua = Stylesheet::default();
    let mut parser = StylesheetStreamParser::new(Origin::UA, 0);
    parser.push_chunk(UA_CSS, &mut ua);
    // Some parsers may return extra buffered rules on finish.
    let extra = parser.finish();
    let mut combined = ua;
    combined.rules.extend(extra.rules);
    combined
}

/// Merge UA and Author stylesheets into a single Stylesheet snapshot.
/// We simply concatenate their rules; cascade will later use origin/source_order.
fn merge_stylesheets(ua: &Stylesheet, author: &Stylesheet) -> Stylesheet {
    let mut merged = Stylesheet::default();
    merged.rules.extend(ua.rules.clone());
    merged.rules.extend(author.rules.clone());
    merged
}

fn default_display_for_tag(tag: &str) -> Display {
    let t = tag.to_ascii_lowercase();
    // Keep this list in sync with UA_CSS above.
    match t.as_str() {
        "style" | "script" => Display::None,
        "div" | "p" | "header" | "main" | "footer" | "section" | "article" | "nav" |
        "ul" | "ol" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "html" | "body" => Display::Block,
        _ => Display::Inline,
    }
}

// Minimal parser for width specified values in inline styles.
// Supports: auto | <number>px | <number>%
fn parse_width_spec(input: &str) -> Option<SizeSpecified> { // deprecated alias
    parse_size_spec(input)
}

fn parse_size_spec(input: &str) -> Option<SizeSpecified> {
    let s = input.trim();
    if s.eq_ignore_ascii_case("auto") {
        return Some(SizeSpecified::Auto);
    }
    if let Some(px_str) = s.strip_suffix("px") {
        let n = px_str.trim().parse::<f32>().ok()?;
        return Some(SizeSpecified::Px(n));
    }
    if let Some(pct_str) = s.strip_suffix('%') {
        let n = pct_str.trim().parse::<f32>().ok()?;
        return Some(SizeSpecified::Percent(n / 100.0));
    }
    // bare number is px
    if let Ok(n) = s.parse::<f32>() {
        return Some(SizeSpecified::Px(n));
    }
    None
}

fn parse_px(input: &str) -> Option<f32> {
    let s = input.trim();
    if s.is_empty() { return None; }
    if s.eq_ignore_ascii_case("0") { return Some(0.0); }
    if let Some(px_str) = s.strip_suffix("px") {
        return px_str.trim().parse::<f32>().ok();
    }
    // Treat bare number as px for inline style convenience
    s.parse::<f32>().ok()
}

fn parse_edges_shorthand(input: &str) -> Option<Edges> {
    let parts: Vec<&str> = input.split_whitespace().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() { return None; }
    let mut e = Edges::default();
    match parts.len() {
        1 => {
            let v = parse_px(parts[0])?; e.top=v; e.right=v; e.bottom=v; e.left=v;
        }
        2 => {
            let vtb = parse_px(parts[0])?; let vlr = parse_px(parts[1])?;
            e.top=vtb; e.bottom=vtb; e.left=vlr; e.right=vlr;
        }
        3 => {
            let vt = parse_px(parts[0])?; let vlr = parse_px(parts[1])?; let vb = parse_px(parts[2])?;
            e.top=vt; e.left=vlr; e.right=vlr; e.bottom=vb;
        }
        _ => {
            let vt = parse_px(parts[0])?; let vr = parse_px(parts[1])?; let vb = parse_px(parts[2])?; let vl = parse_px(parts[3])?;
            e.top=vt; e.right=vr; e.bottom=vb; e.left=vl;
        }
    }
    Some(e)
}
