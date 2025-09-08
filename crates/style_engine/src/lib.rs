use css::parser::StylesheetStreamParser;
use css::ruledb::RuleDB;
use css::rulemap::{index_rules, RuleMap, RuleRef};
use css::selector::{Combinator, ComplexSelector, CompoundSelector, SimpleSelector, Specificity};
use css::types::{Origin, Stylesheet};
use html::dom::NodeKey;
use log::info;
use std::collections::HashMap;
use std::collections::HashSet;

mod computed_style;
mod dom_subscriber;

pub use computed_style::{ColorRGBA, ComputedStyle, Display, Edges, SizeSpecified};

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
        Self {
            author_stylesheet: Stylesheet::default(),
            stylesheet: ua_stylesheet.clone(),
            ruledb: RuleDB::from_stylesheet(&ua_stylesheet),
            ua_stylesheet,
            nodes: HashMap::new(),
            computed: HashMap::new(),
            rule_index: RuleMap::new(),
            matches: HashMap::new(),
            nodes_by_id: HashMap::new(),
            nodes_by_class: HashMap::new(),
            nodes_by_tag: HashMap::new(),
        }
    }

    /// Recompute computed styles for all known nodes in parent-before-child order.
    pub fn recompute_all(&mut self) {
        // Collect nodes and compute depth for ordering
        let mut items: Vec<(usize, NodeKey)> = self
            .nodes
            .keys()
            .cloned()
            .map(|k| (self.node_depth(k), k))
            .collect();
        items.sort_by_key(|(d, _)| *d);
        for (_d, k) in items {
            let cs = self.compute_for_node(k);
            self.computed.insert(k, cs);
        }
    }

    fn node_depth(&self, node: NodeKey) -> usize {
        let mut depth = 0usize;
        let mut cur = Some(node);
        while let Some(k) = cur {
            cur = self.nodes.get(&k).and_then(|ni| ni.parent);
            if cur.is_some() {
                depth += 1;
            }
        }
        depth
    }

    /// Replace the active author stylesheet set with a new snapshot and merge with UA sheet.
    pub fn replace_stylesheet(&mut self, author: Stylesheet) {
        self.author_stylesheet = author;
        self.stylesheet = merge_stylesheets(&self.ua_stylesheet, &self.author_stylesheet);
        // Rebuild RuleDB for the merged stylesheet
        self.ruledb = RuleDB::from_stylesheet(&self.stylesheet);
        // Rebuild rule index and recompute matches for all nodes
        self.rebuild_rule_index();
        self.rematch_all_nodes();
        // Recompute computed styles for all nodes (inheritance-aware)
        self.recompute_all();
        info!(
            "StyleEngine: merged UA+Author stylesheets (ua_rules={}, author_rules={}, indexed_rules={})",
            self.ua_stylesheet.rules.len(),
            self.author_stylesheet.rules.len(),
            self.stylesheet.rules.len()
        );
    }

    /// Read-only access to the current merged stylesheet snapshot.
    pub fn stylesheet(&self) -> &Stylesheet {
        &self.stylesheet
    }

    /// Return a cloned snapshot of computed styles per node (minimal subset for now).
    pub fn computed_snapshot(&self) -> ComputedMap {
        self.computed.clone()
    }

    fn compute_for_node(&self, node: NodeKey) -> ComputedStyle {
        // Gather info and parent style
        let Some(info) = self.nodes.get(&node) else { return ComputedStyle::default(); };
        let parent_style = info.parent.and_then(|p| self.computed.get(&p)).cloned();

        // Working specified values (before inheritance resolution)
        let mut display_spec: Option<Display> = None;
        let mut width_spec: Option<SizeSpecified> = None;
        let mut height_spec: Option<SizeSpecified> = None;
        let mut margin_spec: Edges = Edges::default();
        let mut have_margin = false;
        let mut padding_spec: Edges = Edges::default();
        let mut have_padding = false;
        let mut color_spec: Option<ColorRGBA> = None;
        let mut font_size_spec: Option<f32> = None; // px
        enum LHSrc { Normal, Number(f32), Px(f32) }
        let mut line_height_spec: Option<LHSrc> = None;

        // Apply declarations from matched rules in cascade order
        if let Some(rule_refs) = self.matches.get(&node) {
            // Build sortable list of (important, origin, specificity, source_order, prop, value)
            let mut items: Vec<(bool, Origin, Specificity, u32, String, String)> = Vec::new();
            for rr in rule_refs {
                if let Some(rule) = self.stylesheet.rules.get(rr.rule_idx)
                    && let Some(sel) = rule.selectors.get(rr.selector_idx)
                {
                    for d in &rule.declarations {
                        items.push((d.important, rule.origin, sel.specificity, rule.source_order, d.name.to_ascii_lowercase(), d.value.clone()));
                    }
                }
            }
            // Sort ascending so later items override
            items.sort_by(|a, b| a.cmp(b));
            for (_imp, _origin, _spec, _ord, prop, val_raw) in items {
                let val = val_raw.trim();
                match prop.as_str() {
                    "display" => {
                        let v = val.to_ascii_lowercase();
                        display_spec = match v.as_str() {
                            "none" => Some(Display::None),
                            "block" => Some(Display::Block),
                            "inline" => Some(Display::Inline),
                            _ => display_spec,
                        };
                    }
                    "width" => { width_spec = parse_size_spec(val).or(width_spec); }
                    "height" => { height_spec = parse_size_spec(val).or(height_spec); }
                    "margin" => { if let Some(e) = parse_edges_shorthand(val) { margin_spec = e; have_margin = true; } }
                    "padding" => { if let Some(e) = parse_edges_shorthand(val) { padding_spec = e; have_padding = true; } }
                    "margin-top" => { if let Some(px) = parse_px(val) { margin_spec.top = px; have_margin = true; } }
                    "margin-right" => { if let Some(px) = parse_px(val) { margin_spec.right = px; have_margin = true; } }
                    "margin-bottom" => { if let Some(px) = parse_px(val) { margin_spec.bottom = px; have_margin = true; } }
                    "margin-left" => { if let Some(px) = parse_px(val) { margin_spec.left = px; have_margin = true; } }
                    "padding-top" => { if let Some(px) = parse_px(val) { padding_spec.top = px; have_padding = true; } }
                    "padding-right" => { if let Some(px) = parse_px(val) { padding_spec.right = px; have_padding = true; } }
                    "padding-bottom" => { if let Some(px) = parse_px(val) { padding_spec.bottom = px; have_padding = true; } }
                    "padding-left" => { if let Some(px) = parse_px(val) { padding_spec.left = px; have_padding = true; } }
                    "color" => { if let Some(c) = Self::parse_color(val) { color_spec = Some(c); } }
                    "font-size" => { if let Some(px) = parse_px(val) { font_size_spec = Some(px); } }
                    "line-height" => {
                        let v = val.to_ascii_lowercase();
                        if v == "normal" { line_height_spec = Some(LHSrc::Normal); }
                        else if let Ok(n) = v.parse::<f32>() { line_height_spec = Some(LHSrc::Number(n)); }
                        else if let Some(px) = parse_px(&v) { line_height_spec = Some(LHSrc::Px(px)); }
                    }
                    _ => {}
                }
            }
        }

        // Inline style overrides (Author, highest priority)
        if let Some(d) = info.inline_display { display_spec = Some(d); }
        if let Some(w) = info.inline_width { width_spec = Some(w); }
        if let Some(h) = info.inline_height { height_spec = Some(h); }
        if let Some(m) = info.inline_margin { margin_spec = m; have_margin = true; }
        if let Some(p) = info.inline_padding { padding_spec = p; have_padding = true; }

        // Build computed style with inheritance
        let mut cs = ComputedStyle::default();
        // display: default fallback by tag if still unspecified
        cs.display = display_spec.unwrap_or_else(|| default_display_for_tag(&info.tag));
        // box model
        if have_margin { cs.margin = margin_spec; }
        if have_padding { cs.padding = padding_spec; }
        cs.width = width_spec.unwrap_or(SizeSpecified::Auto);
        cs.height = height_spec.unwrap_or(SizeSpecified::Auto);
        // inherited properties
        if let Some(ps) = parent_style.as_ref() {
            cs.color = ps.color;
            cs.font_size = ps.font_size;
            cs.line_height = ps.line_height;
        }
        if let Some(c) = color_spec { cs.color = c; }
        if let Some(fs) = font_size_spec { cs.font_size = fs; }
        if let Some(lh) = line_height_spec {
            cs.line_height = match lh {
                LHSrc::Normal => 1.2,
                LHSrc::Number(n) => n,
                LHSrc::Px(px) => {
                    let fs = if let Some(fs) = font_size_spec { fs } else { cs.font_size };
                    if fs > 0.0 { px / fs } else { 1.2 }
                }
            };
        }
        cs
    }

    fn parse_color(input: &str) -> Option<ColorRGBA> {
        let s = input.trim();
        if s.starts_with('#') {
            let hex = &s[1..];
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some(ColorRGBA { r, g, b, a: 255 });
            }
        }
        match s.to_ascii_lowercase().as_str() {
            "black" => Some(ColorRGBA { r: 0, g: 0, b: 0, a: 255 }),
            "red" => Some(ColorRGBA { r: 255, g: 0, b: 0, a: 255 }),
            "green" => Some(ColorRGBA { r: 0, g: 128, b: 0, a: 255 }),
            "blue" => Some(ColorRGBA { r: 0, g: 0, b: 255, a: 255 }),
            _ => None,
        }
    }

    fn rebuild_rule_index(&mut self) {
        let mut map = RuleMap::new();
        index_rules(&self.stylesheet, &mut map);
        self.rule_index = map;
    }

    fn rematch_all_nodes(&mut self) {
        let keys: Vec<NodeKey> = self.nodes.keys().cloned().collect();
        for k in keys {
            self.rematch_node(k);
        }
    }

    fn add_matching_rules(
        rules: &Vec<RuleRef>,
        seen: &mut HashSet<(usize, usize)>,
    ) -> Vec<RuleRef> {
        rules
            .iter()
            .filter(|rule_ref| seen.insert((rule_ref.rule_idx, rule_ref.selector_idx)))
            .cloned()
            .collect()
    }

    fn rematch_node(&mut self, node: NodeKey) {
        // Build candidates from rule_index using id, classes, tag and universal
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        let mut cands: Vec<RuleRef> = Vec::new();
        if let Some(info) = self.nodes.get(&node) {
            if let Some(id) = info.id.as_ref()
                && let Some(rules) = self.rule_index.by_id.get(id)
            {
                cands.append(&mut Self::add_matching_rules(rules, &mut seen));
            }
            for class in &info.classes {
                if let Some(rules) = self.rule_index.by_class.get(class) {
                    cands.append(&mut Self::add_matching_rules(rules, &mut seen));
                }
            }
            let tag_lc = info.tag.to_ascii_lowercase();
            if let Some(rules) = self.rule_index.by_tag.get(&tag_lc) {
                cands.append(&mut Self::add_matching_rules(rules, &mut seen));
            }

            cands.append(&mut Self::add_matching_rules(
                &self.rule_index.universal,
                &mut seen,
            ));
        }
        // Filter by full selector match
        let mut matched: Vec<RuleRef> = Vec::new();
        for rr in cands {
            if let Some(rule) = self.stylesheet.rules.get(rr.rule_idx)
                && let Some(sel) = rule.selectors.get(rr.selector_idx)
                && self.match_complex_selector(node, sel)
            {
                matched.push(rr);
            }
        }
        self.matches.insert(node, matched);
    }

    fn match_complex_selector(&self, node: NodeKey, sel: &ComplexSelector) -> bool {
        if sel.sequence.is_empty() {
            return false;
        }
        // Start from rightmost compound
        let mut current = node;
        let mut idx: isize = sel.sequence.len() as isize - 1;
        // Ensure rightmost compound matches the node
        let (last_comp, _) = &sel.sequence[idx as usize];
        if !self.match_compound(current, last_comp) {
            return false;
        }
        while idx > 0 {
            let (comp, comb_opt) = &sel.sequence[(idx - 1) as usize];
            let comb = comb_opt.unwrap_or(Combinator::Descendant);
            match comb {
                Combinator::Descendant => {
                    // climb ancestors to find a match
                    let mut p = self.nodes.get(&current).and_then(|ni| ni.parent);
                    let mut found = false;
                    while let Some(anc) = p {
                        if self.match_compound(anc, comp) {
                            current = anc;
                            found = true;
                            break;
                        }
                        p = self.nodes.get(&anc).and_then(|ni| ni.parent);
                    }
                    if !found {
                        return false;
                    }
                }
                Combinator::Child => {
                    let p = self.nodes.get(&current).and_then(|ni| ni.parent);
                    if let Some(anc) = p {
                        if self.match_compound(anc, comp) {
                            current = anc;
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
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
        let Some(info) = self.nodes.get(&node) else {
            return false;
        };
        for s in &comp.simples {
            match s {
                SimpleSelector::Universal => {}
                SimpleSelector::Type(t) => {
                    if info.tag.eq_ignore_ascii_case(t) == false {
                        return false;
                    }
                }
                SimpleSelector::Id(id) => {
                    if info.id.as_ref().map(|v| v == id).unwrap_or(false) == false {
                        return false;
                    }
                }
                SimpleSelector::Class(c) => {
                    if !info.classes.contains(c) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn update_id_index(&mut self, node: NodeKey, old_id: Option<String>, new_id: Option<String>) {
        if let Some(old_id) = old_id
            && let Some(rules) = self.nodes_by_id.get_mut(&old_id)
        {
            rules.retain(|key| *key != node);
        }
        if let Some(new_id) = new_id {
            self.nodes_by_id.entry(new_id).or_default().push(node);
        }
    }

    fn update_class_index(&mut self, node: NodeKey, old: &HashSet<String>, new: &HashSet<String>) {
        for c in old {
            if !new.contains(c) {
                if let Some(v) = self.nodes_by_class.get_mut(c) {
                    v.retain(|k| *k != node);
                }
            }
        }
        for c in new {
            if !old.contains(c) {
                self.nodes_by_class.entry(c.clone()).or_default().push(node);
            }
        }
    }

    fn add_tag_index(&mut self, node: NodeKey, tag: &str) {
        self.nodes_by_tag
            .entry(tag.to_ascii_lowercase())
            .or_default()
            .push(node);
    }

    fn remove_node_recursive(&mut self, node: NodeKey) {
        if let Some(info) = self.nodes.remove(&node) {
            // remove from parent children
            if let Some(p) = info.parent {
                if let Some(pi) = self.nodes.get_mut(&p) {
                    pi.children.retain(|k| *k != node);
                }
            }
            // drop indexes
            if let Some(idv) = info.id {
                if let Some(v) = self.nodes_by_id.get_mut(&idv) {
                    v.retain(|k| *k != node);
                }
            }
            for c in info.classes {
                if let Some(v) = self.nodes_by_class.get_mut(&c) {
                    v.retain(|k| *k != node);
                }
            }
            // tag index
            if let Some(v) = self.nodes_by_tag.get_mut(&info.tag.to_ascii_lowercase()) {
                v.retain(|k| *k != node);
            }
            self.matches.remove(&node);
            self.computed.remove(&node);
            // recurse
            for ch in info.children {
                self.remove_node_recursive(ch);
            }
        }
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
        "div" | "p" | "header" | "main" | "footer" | "section" | "article" | "nav" | "ul"
        | "ol" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "html" | "body" => Display::Block,
        _ => Display::Inline,
    }
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
    if s.is_empty() {
        return None;
    }
    if s.eq_ignore_ascii_case("0") {
        return Some(0.0);
    }
    if let Some(px_str) = s.strip_suffix("px") {
        return px_str.trim().parse::<f32>().ok();
    }
    // Treat bare number as px for inline style convenience
    s.parse::<f32>().ok()
}

fn parse_edges_shorthand(input: &str) -> Option<Edges> {
    let parts: Vec<&str> = input.split_whitespace().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    let mut e = Edges::default();
    match parts.len() {
        1 => {
            let v = parse_px(parts[0])?;
            e.top = v;
            e.right = v;
            e.bottom = v;
            e.left = v;
        }
        2 => {
            let vtb = parse_px(parts[0])?;
            let vlr = parse_px(parts[1])?;
            e.top = vtb;
            e.bottom = vtb;
            e.left = vlr;
            e.right = vlr;
        }
        3 => {
            let vt = parse_px(parts[0])?;
            let vlr = parse_px(parts[1])?;
            let vb = parse_px(parts[2])?;
            e.top = vt;
            e.left = vlr;
            e.right = vlr;
            e.bottom = vb;
        }
        _ => {
            let vt = parse_px(parts[0])?;
            let vr = parse_px(parts[1])?;
            let vb = parse_px(parts[2])?;
            let vl = parse_px(parts[3])?;
            e.top = vt;
            e.right = vr;
            e.bottom = vb;
            e.left = vl;
        }
    }
    Some(e)
}
