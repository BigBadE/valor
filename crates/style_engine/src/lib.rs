use css::parser::StylesheetStreamParser;
use css::ruledb::RuleDB;
use css::rulemap::{RuleMap, RuleRef, index_rules};
use css::selector::{
    Combinator, ComplexSelector, CompoundSelector, PseudoClass, SimpleSelector, Specificity,
};
use css::types::{Declaration, Origin, Stylesheet};
use csscolorparser::Color as CssColor;
use js::NodeKey;
use log::{info, warn};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::hash::{Hasher, Hash};
use std::collections::hash_map::DefaultHasher;

mod computed_style;
mod dom_subscriber;
mod used_values;

pub use used_values::{UsedValues, UsedValuesContext, resolve_used_values};

pub use computed_style::{ColorRGBA, ComputedStyle, Display, Edges, SizeSpecified, BorderStyle, FontStyle, Position, AlignItems, Overflow};

/// Internal node info tracked by the StyleEngine mirror for minimal style computation.
#[derive(Debug, Clone)]
struct NodeInfo {
    tag: String,
    id: Option<String>,
    classes: HashSet<String>,
    /// Lowercased attribute name to value mapping (excludes id/class/style which have dedicated fields)
    attributes: HashMap<String, String>,
    parent: Option<NodeKey>,
    children: Vec<NodeKey>,
}

pub type ComputedMap = HashMap<NodeKey, ComputedStyle>;

/// StyleEngine is a DOM subscriber that will own selector matching,
/// cascade, and computed style generation in future steps.
/// For now it installs a minimal UA stylesheet, accepts Author Stylesheet updates,
/// merges them, and mirrors DOM updates while computing a very small subset:
/// - display defaults by tag (UA)
/// - inline style attribute display override (Author)
/// Internal cache for sharing identical ComputedStyle instances across nodes.
/// Uses a structural hash to bucket candidates and stores canonical Arc<ComputedStyle> values.
struct StyleCache {
    arena: Vec<Arc<ComputedStyle>>,
    index: HashMap<u64, Vec<usize>>, // hash -> indices into arena
}

impl StyleCache {
    /// Create a new, empty style cache.
    fn new() -> Self { Self { arena: Vec::new(), index: HashMap::new() } }

    /// Compute a stable structural hash for a ComputedStyle.
    fn hash_style(style: &ComputedStyle) -> u64 {
        let mut hasher = DefaultHasher::new();
        // enums
        hasher.write_u8(match style.display { Display::None => 0, Display::Block => 1, Display::Inline => 2, Display::Flex => 3, Display::InlineFlex => 4 });
        hasher.write_u8(match style.position { Position::Static => 0, Position::Relative => 1, Position::Absolute => 2, Position::Fixed => 3, Position::Sticky => 4 });
        hasher.write_u8(match style.align_items { AlignItems::Stretch => 0, AlignItems::FlexStart => 1, AlignItems::FlexEnd => 2, AlignItems::Center => 3, AlignItems::Baseline => 4 });
        // edges
        for v in [style.margin.top, style.margin.right, style.margin.bottom, style.margin.left] { hasher.write(&v.to_bits().to_le_bytes()); }
        for v in [style.padding.top, style.padding.right, style.padding.bottom, style.padding.left] { hasher.write(&v.to_bits().to_le_bytes()); }
        for v in [style.border_width.top, style.border_width.right, style.border_width.bottom, style.border_width.left] { hasher.write(&v.to_bits().to_le_bytes()); }
        // border style/color
        hasher.write_u8(match style.border_style { BorderStyle::None => 0, BorderStyle::Solid => 1, BorderStyle::Dashed => 2, BorderStyle::Dotted => 3 });
        hasher.write(&[style.border_color.red, style.border_color.green, style.border_color.blue, style.border_color.alpha]);
        hasher.write(&[style.background_color.red, style.background_color.green, style.background_color.blue, style.background_color.alpha]);
        // text
        hasher.write(&style.color.red.to_le_bytes()); // note: u8->to_le_bytes returns [u8;1] but stable; fallback directly writing bytes
        hasher.write(&[style.color.red, style.color.green, style.color.blue, style.color.alpha]);
        hasher.write(&style.font_size.to_bits().to_le_bytes());
        hasher.write(&style.line_height.to_bits().to_le_bytes());
        hasher.write(&style.font_weight.to_le_bytes());
        hasher.write_u8(match style.font_style { FontStyle::Normal => 0, FontStyle::Italic => 1, FontStyle::Oblique => 2 });
        hasher.write(style.font_family.as_bytes());
        // sizes
        fn hash_size(h: &mut DefaultHasher, s: &SizeSpecified) {
            match s { SizeSpecified::Auto => h.write_u8(0), SizeSpecified::Px(px) => { h.write_u8(1); h.write(&px.to_bits().to_le_bytes()); }, SizeSpecified::Percent(p) => { h.write_u8(2); h.write(&p.to_bits().to_le_bytes()); } }
        }
        hash_size(&mut hasher, &style.width);
        hash_size(&mut hasher, &style.height);
        // positioned offsets
        if let Some(v) = &style.top { hasher.write_u8(1); hash_size(&mut hasher, v); } else { hasher.write_u8(0); }
        if let Some(v) = &style.right { hasher.write_u8(1); hash_size(&mut hasher, v); } else { hasher.write_u8(0); }
        if let Some(v) = &style.bottom { hasher.write_u8(1); hash_size(&mut hasher, v); } else { hasher.write_u8(0); }
        if let Some(v) = &style.left { hasher.write_u8(1); hash_size(&mut hasher, v); } else { hasher.write_u8(0); }
        // min/max
        if let Some(mw) = &style.min_width { hasher.write_u8(1); hash_size(&mut hasher, mw); } else { hasher.write_u8(0); }
        if let Some(mxw) = &style.max_width { hasher.write_u8(1); hash_size(&mut hasher, mxw); } else { hasher.write_u8(0); }
        if let Some(mh) = &style.min_height { hasher.write_u8(1); hash_size(&mut hasher, mh); } else { hasher.write_u8(0); }
        if let Some(mxh) = &style.max_height { hasher.write_u8(1); hash_size(&mut hasher, mxh); } else { hasher.write_u8(0); }
        // overflow
        hasher.write_u8(match style.overflow { computed_style::Overflow::Visible => 0, computed_style::Overflow::Hidden => 1, computed_style::Overflow::Scroll => 2, computed_style::Overflow::Auto => 3 });
        // flex
        hasher.write(&style.flex_grow.to_bits().to_le_bytes());
        hasher.write(&style.flex_shrink.to_bits().to_le_bytes());
        hash_size(&mut hasher, &style.flex_basis);
        // custom properties (order independent): hash sorted by key
        let mut items: Vec<(&String, &String)> = style.custom_properties.iter().collect();
        items.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in items { hasher.write(k.as_bytes()); hasher.write(v.as_bytes()); }
        hasher.finish()
    }

    /// Intern the provided style, returning a shared Arc to the canonical instance.
    fn intern(&mut self, style: ComputedStyle) -> Arc<ComputedStyle> {
        let hash = Self::hash_style(&style);
        if let Some(indices) = self.index.get(&hash) {
            for &idx in indices {
                if let Some(existing) = self.arena.get(idx) {
                    if **existing == style {
                        return existing.clone();
                    }
                }
            }
        }
        let arc = Arc::new(style);
        let idx = self.arena.len();
        self.arena.push(arc.clone());
        self.index.entry(hash).or_default().push(idx);
        arc
    }
}

pub struct StyleEngine {
    ua_stylesheet: Stylesheet,
    author_stylesheet: Stylesheet,
    stylesheet: Stylesheet,
    ruledb: RuleDB,
    nodes: HashMap<NodeKey, NodeInfo>,
    computed: HashMap<NodeKey, Arc<ComputedStyle>>,
    style_cache: StyleCache,
    rule_index: RuleMap,
    matches: HashMap<NodeKey, Vec<RuleRef>>,
    nodes_by_id: HashMap<String, Vec<NodeKey>>,
    nodes_by_class: HashMap<String, Vec<NodeKey>>,
    nodes_by_tag: HashMap<String, Vec<NodeKey>>,
    inline_decls: HashMap<NodeKey, Vec<Declaration>>,
    /// Nodes that need recomputation at the next batch flush.
    dirty_nodes: HashSet<NodeKey>,
    /// Selectors we've already warned about for unsupported features, keyed by (rule_idx, selector_idx).
    warned_selectors: HashSet<(usize, usize)>,
    /// Sticky flag indicating whether computed styles changed since last check.
    style_changed: bool,
    /// Increments whenever the active stylesheet set changes.
    rules_epoch: u64,
    /// Per-node epoch indicating when matches were last computed.
    node_match_epoch: HashMap<NodeKey, u64>,
    /// Performance counters for recompute_dirty invocations.
    last_dirty_recompute_count: u64,
    total_dirty_recompute_count: u64,
    /// Nodes whose computed styles changed in the last recompute_dirty; drained by take_changed_nodes().
    changed_nodes: HashSet<NodeKey>,
}

impl StyleEngine {
    /// Create a new StyleEngine with a built-in user-agent stylesheet and empty author stylesheet.
    pub fn new() -> Self {
        let ua_stylesheet = build_ua_stylesheet();
        Self {
            author_stylesheet: Stylesheet::default(),
            stylesheet: ua_stylesheet.clone(),
            ruledb: RuleDB::from_stylesheet(&ua_stylesheet),
            ua_stylesheet,
            nodes: HashMap::new(),
            computed: HashMap::new(),
            style_cache: StyleCache::new(),
            rule_index: RuleMap::new(),
            matches: HashMap::new(),
            nodes_by_id: HashMap::new(),
            nodes_by_class: HashMap::new(),
            nodes_by_tag: HashMap::new(),
            inline_decls: HashMap::new(),
            dirty_nodes: HashSet::new(),
            warned_selectors: HashSet::new(),
            style_changed: false,
            rules_epoch: 0,
            node_match_epoch: HashMap::new(),
            last_dirty_recompute_count: 0,
            total_dirty_recompute_count: 0,
            changed_nodes: HashSet::new(),
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
            let interned = self.style_cache.intern(cs);
            self.computed.insert(k, interned);
        }
    }

    /// Mark a node as dirty for later recomputation.
    pub fn mark_dirty(&mut self, node: NodeKey) {
        self.dirty_nodes.insert(node);
    }

    /// Mark a node and all descendants as dirty.
    pub fn mark_subtree_dirty(&mut self, node: NodeKey) {
        // Collect subtree first using only immutable borrows, then mark all at once.
        let mut stack: Vec<NodeKey> = vec![node];
        let mut collected: Vec<NodeKey> = Vec::new();
        while let Some(cur) = stack.pop() {
            collected.push(cur);
            if let Some(info) = self.nodes.get(&cur) {
                for child in &info.children {
                    stack.push(*child);
                }
            }
        }
        for k in collected {
            self.dirty_nodes.insert(k);
        }
    }

    /// Recompute all nodes currently marked as dirty in parent-before-child order and clear the set.
    pub fn recompute_dirty(&mut self) {
        if self.dirty_nodes.is_empty() {
            self.last_dirty_recompute_count = 0;
            return;
        }
        // Order dirty nodes by depth so parents are computed before children for inheritance.
        let mut items: Vec<(usize, NodeKey)> = self
            .dirty_nodes
            .iter()
            .cloned()
            .map(|k| (self.node_depth(k), k))
            .collect();
        items.sort_by_key(|(d, _)| *d);
        self.last_dirty_recompute_count = items.len() as u64;
        self.total_dirty_recompute_count = self
            .total_dirty_recompute_count
            .saturating_add(self.last_dirty_recompute_count);
        let mut any_changed = false;
        self.changed_nodes.clear();
        for (_d, k) in items {
            let new_cs = self.compute_for_node(k);
            let changed = match self.computed.get(&k) {
                Some(old) => **old != new_cs,
                None => true,
            };
            if changed {
                any_changed = true;
                self.changed_nodes.insert(k);
            }
            let interned = self.style_cache.intern(new_cs);
            self.computed.insert(k, interned);
        }
        if any_changed {
            self.style_changed = true;
        }
        self.dirty_nodes.clear();
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
        // Increment rules epoch so caches invalidate lazily
        self.rules_epoch = self.rules_epoch.wrapping_add(1);
        // Rebuild rule index and rematch only affected nodes when possible
        self.rebuild_rule_index();
        self.targeted_rematch_after_stylesheet_update();
        // Recompute computed styles for dirty nodes (inheritance-aware)
        self.recompute_dirty();
        info!(
            "StyleEngine: merged UA+Author stylesheets (ua_rules={}, author_rules={}, indexed_rules={})",
            self.ua_stylesheet.rules.len(),
            self.author_stylesheet.rules.len(),
            self.stylesheet.rules.len()
        );
    }

    /// Read-only access to the current merged stylesheet snapshot.
    pub fn perf_last_dirty_recompute_count(&self) -> u64 {
        self.last_dirty_recompute_count
    }
    /// Cumulative dirty recompute count across batches.
    pub fn perf_total_dirty_recompute_count(&self) -> u64 {
        self.total_dirty_recompute_count
    }
    /// Current rules epoch value.
    pub fn current_rules_epoch(&self) -> u64 {
        self.rules_epoch
    }

    /// Read-only access to the current merged stylesheet snapshot.
    pub fn stylesheet(&self) -> &Stylesheet {
        &self.stylesheet
    }

    /// Return a cloned snapshot of computed styles per node (minimal subset for now).
    pub fn computed_snapshot(&self) -> ComputedMap {
        let mut out: ComputedMap = HashMap::with_capacity(self.computed.len());
        for (k, v) in &self.computed {
            out.insert(*k, (**v).clone());
        }
        out
    }

    /// Resolve the first NodeKey associated with the given element id, if any.
    pub fn resolve_first_node_by_id(&self, id: &str) -> Option<NodeKey> {
        self.nodes_by_id.get(id).and_then(|v| v.first().copied())
    }

    /// Return whether styles changed since the last check and clear the flag.
    pub fn take_and_clear_style_changed(&mut self) -> bool {
        let changed = self.style_changed;
        self.style_changed = false;
        changed
    }

    /// Drain and return the set of nodes whose computed styles changed during the last recomputation.
    pub fn take_changed_nodes(&mut self) -> Vec<NodeKey> {
        let out: Vec<NodeKey> = self.changed_nodes.iter().cloned().collect();
        self.changed_nodes.clear();
        out
    }

    fn compute_for_node(&self, node: NodeKey) -> ComputedStyle {
        // Gather info and parent style
        let Some(info) = self.nodes.get(&node) else {
            return ComputedStyle::default();
        };
        let parent_style = info.parent.and_then(|p| self.computed.get(&p)).cloned();

        // Working specified values (before inheritance resolution)
        let mut display_spec: Option<Display> = None;
        let mut width_spec: Option<SizeSpecified> = None;
        let mut height_spec: Option<SizeSpecified> = None;
        let mut min_width_spec: Option<SizeSpecified> = None;
        let mut max_width_spec: Option<SizeSpecified> = None;
        let mut min_height_spec: Option<SizeSpecified> = None;
        let mut max_height_spec: Option<SizeSpecified> = None;
        let mut margin_spec: Edges = Edges::default();
        let mut have_margin = false;
        let mut padding_spec: Edges = Edges::default();
        let mut have_padding = false;
        let mut color_spec: Option<ColorRGBA> = None;
        let mut font_size_spec: Option<f32> = None; // px
        enum LHSrc {
            Normal,
            Number(f32),
            Px(f32),
        }
        let mut line_height_spec: Option<LHSrc> = None;
        // Box/border
        let mut border_width_spec: Edges = Edges::default();
        let mut have_border_width = false;
        let mut border_style_spec: Option<BorderStyle> = None;
        let mut border_color_spec: Option<ColorRGBA> = None;
        let mut background_color_spec: Option<ColorRGBA> = None;
        let mut position_spec: Option<Position> = None;
        // Positioned offsets + overflow
        let mut top_spec: Option<SizeSpecified> = None;
        let mut right_spec: Option<SizeSpecified> = None;
        let mut bottom_spec: Option<SizeSpecified> = None;
        let mut left_spec: Option<SizeSpecified> = None;
        let mut overflow_spec: Option<computed_style::Overflow> = None;
        // Flex properties
        let mut flex_grow_spec: Option<f32> = None;
        let mut flex_shrink_spec: Option<f32> = None;
        let mut flex_basis_spec: Option<SizeSpecified> = None;
        let mut align_items_spec: Option<AlignItems> = None;
        // Text
        let mut font_weight_spec: Option<u16> = None;
        let mut font_style_spec: Option<FontStyle> = None;
        let mut font_family_spec: Option<String> = None;
        // CSS custom properties environment (inherited by default)
        let mut var_env: HashMap<String, String> = parent_style
            .as_ref()
            .map(|ps| ps.custom_properties.clone())
            .unwrap_or_default();

        // Apply declarations from matched rules and inline style in cascade order
        let mut items: Vec<(u8, u8, Specificity, u32, String, String)> = Vec::new();
        if let Some(rule_refs) = self.matches.get(&node) {
            for rr in rule_refs {
                if let Some(rule) = self.stylesheet.rules.get(rr.rule_idx)
                    && let Some(sel) = rule.selectors.get(rr.selector_idx)
                {
                    for d in &rule.declarations {
                        let importance_rank: u8 = if d.important { 1 } else { 0 }; // important later
                        let origin_rank: u8 = if d.important {
                            match rule.origin {
                                // important: UA < Author < User
                                Origin::UA => 0,
                                Origin::Author => 1,
                                Origin::User => 2,
                            }
                        } else {
                            match rule.origin {
                                // normal: UA < User < Author
                                Origin::UA => 0,
                                Origin::User => 1,
                                Origin::Author => 2,
                            }
                        };
                        items.push((
                            importance_rank,
                            origin_rank,
                            sel.specificity,
                            rule.source_order,
                            d.name.to_ascii_lowercase(),
                            d.value.clone(),
                        ));
                    }
                }
            }
        }
        // Inline style: treat as author pseudo-rule with highest author bucket and max specificity
        if let Some(decls) = self.inline_decls.get(&node) {
            for d in decls {
                let importance_rank: u8 = if d.important { 1 } else { 0 };
                let origin_rank: u8 = if d.important { 1 } else { 2 }; // inline: important rank like Author important, normal above Author normal via specificity
                items.push((
                    importance_rank,
                    origin_rank,
                    Specificity(u32::MAX),
                    u32::MAX,
                    d.name.to_ascii_lowercase(),
                    d.value.clone(),
                ));
            }
        }
        // Sort ascending so later items override
        items.sort_by(|a, b| a.cmp(b));
        for (_imp_rank, _origin_rank, _spec, _ord, prop, val_raw) in items {
            let prop_lc = prop.to_ascii_lowercase();
            // Custom property declaration: store specified tokens; resolution at use-sites
            if prop_lc.starts_with("--") {
                var_env.insert(prop_lc.clone(), val_raw.trim().to_string());
                continue;
            }
            // Resolve var() for known properties before parsing
            let resolved_opt = Self::resolve_vars(val_raw.trim(), &var_env);
            let val_resolved: String;
            if let Some(resolved) = resolved_opt {
                val_resolved = resolved;
            } else {
                // Unresolved var() without fallback makes declaration invalid
                continue;
            }
            let val = val_resolved.as_str();
            match prop_lc.as_str() {
                "display" => {
                    let v = val.to_ascii_lowercase();
                    display_spec = match v.as_str() {
                        "none" => Some(Display::None),
                        "block" => Some(Display::Block),
                        "inline" => Some(Display::Inline),
                        "flex" => Some(Display::Flex),
                        "inline-flex" => Some(Display::InlineFlex),
                        _ => display_spec,
                    };
                }
                "position" => {
                    let v = val.to_ascii_lowercase();
                    position_spec = Some(match v.as_str() {
                        "relative" => Position::Relative,
                        "absolute" => Position::Absolute,
                        "fixed" => Position::Fixed,
                        "sticky" => Position::Sticky,
                        _ => Position::Static,
                    });
                }
                "top" => { top_spec = parse_size_spec(val).or(top_spec); }
                "right" => { right_spec = parse_size_spec(val).or(right_spec); }
                "bottom" => { bottom_spec = parse_size_spec(val).or(bottom_spec); }
                "left" => { left_spec = parse_size_spec(val).or(left_spec); }
                "overflow" => {
                    let v = val.to_ascii_lowercase();
                    overflow_spec = Some(match v.as_str() {
                        "hidden" => computed_style::Overflow::Hidden,
                        "scroll" => computed_style::Overflow::Scroll,
                        "auto" => computed_style::Overflow::Auto,
                        _ => computed_style::Overflow::Visible,
                    });
                }
                "width" => { width_spec = parse_size_spec(val).or(width_spec); }
                "height" => { height_spec = parse_size_spec(val).or(height_spec); }
                "min-width" => { min_width_spec = parse_size_spec(val).or(min_width_spec); }
                "max-width" => { max_width_spec = parse_size_spec(val).or(max_width_spec); }
                "min-height" => { min_height_spec = parse_size_spec(val).or(min_height_spec); }
                "max-height" => { max_height_spec = parse_size_spec(val).or(max_height_spec); }
                "margin" => {
                    if let Some(e) = parse_edges_shorthand(val) { margin_spec = e; have_margin = true; }
                }
                "padding" => {
                    if let Some(e) = parse_edges_shorthand(val) { padding_spec = e; have_padding = true; }
                }
                "margin-top" => { if let Some(px) = parse_px(val) { margin_spec.top = px; have_margin = true; } }
                "margin-right" => { if let Some(px) = parse_px(val) { margin_spec.right = px; have_margin = true; } }
                "margin-bottom" => { if let Some(px) = parse_px(val) { margin_spec.bottom = px; have_margin = true; } }
                "margin-left" => { if let Some(px) = parse_px(val) { margin_spec.left = px; have_margin = true; } }
                "padding-top" => { if let Some(px) = parse_px(val) { padding_spec.top = px; have_padding = true; } }
                "padding-right" => { if let Some(px) = parse_px(val) { padding_spec.right = px; have_padding = true; } }
                "padding-bottom" => { if let Some(px) = parse_px(val) { padding_spec.bottom = px; have_padding = true; } }
                "padding-left" => { if let Some(px) = parse_px(val) { padding_spec.left = px; have_padding = true; } }
                // Border widths (uniform and per-side longhands)
                "border-width" => {
                    if let Some(px) = parse_px(val) { border_width_spec.top = px; border_width_spec.right = px; border_width_spec.bottom = px; border_width_spec.left = px; have_border_width = true; }
                }
                "border-top-width" => { if let Some(px) = parse_px(val) { border_width_spec.top = px; have_border_width = true; } }
                "border-right-width" => { if let Some(px) = parse_px(val) { border_width_spec.right = px; have_border_width = true; } }
                "border-bottom-width" => { if let Some(px) = parse_px(val) { border_width_spec.bottom = px; have_border_width = true; } }
                "border-left-width" => { if let Some(px) = parse_px(val) { border_width_spec.left = px; have_border_width = true; } }
                // Border style (uniform or per-side collapsed to uniform)
                "border-style" => {
                    let v = val.to_ascii_lowercase();
                    border_style_spec = Some(match v.as_str() { "none" => BorderStyle::None, "dashed" => BorderStyle::Dashed, "dotted" => BorderStyle::Dotted, _ => BorderStyle::Solid });
                }
                "border-top-style" | "border-right-style" | "border-bottom-style" | "border-left-style" => {
                    let v = val.to_ascii_lowercase();
                    border_style_spec = Some(match v.as_str() { "none" => BorderStyle::None, "dashed" => BorderStyle::Dashed, "dotted" => BorderStyle::Dotted, _ => BorderStyle::Solid });
                }
                // Border color (uniform or per-side collapsed to last specified)
                "border-color" | "border-top-color" | "border-right-color" | "border-bottom-color" | "border-left-color" => {
                    if let Some(c) = Self::parse_color(val) { border_color_spec = Some(c); }
                }
                // Background
                "background-color" => { if let Some(c) = Self::parse_color(val) { background_color_spec = Some(c); } }
                "color" => { if let Some(c) = Self::parse_color(val) { color_spec = Some(c); } }
                // Font
                "font-size" => {
                    // Support px, bare number (px), em/ex relative to parent font-size
                    let parent_fs = parent_style.as_ref().map(|ps| ps.font_size).unwrap_or(16.0);
                    let v = val.to_ascii_lowercase();
                    let parsed_px = if let Some(px) = parse_px(&v) {
                        Some(px)
                    } else if let Some(em_str) = v.strip_suffix("em") {
                        if let Ok(n) = em_str.trim().parse::<f32>() { Some(n * parent_fs) } else { None }
                    } else if let Some(ex_str) = v.strip_suffix("ex") {
                        if let Ok(n) = ex_str.trim().parse::<f32>() { Some(n * parent_fs * 0.5) } else { None }
                    } else { None };
                    if let Some(px) = parsed_px { font_size_spec = Some(px); }
                }
                "font-weight" => {
                    let v = val.to_ascii_lowercase();
                    font_weight_spec = if v == "bold" { Some(700) } else if v == "normal" { Some(400) } else { v.parse::<u16>().ok() };
                }
                "font-style" => {
                    let v = val.to_ascii_lowercase();
                    font_style_spec = Some(match v.as_str() { "italic" => FontStyle::Italic, "oblique" => FontStyle::Oblique, _ => FontStyle::Normal });
                }
                "font-family" => { font_family_spec = Some(val.to_string()); }
                "line-height" => {
                    let v = val.to_ascii_lowercase();
                    if v == "normal" { line_height_spec = Some(LHSrc::Normal); }
                    else if let Ok(n) = v.parse::<f32>() { line_height_spec = Some(LHSrc::Number(n)); }
                    else if let Some(px) = parse_px(&v) { line_height_spec = Some(LHSrc::Px(px)); }
                }
                // Flexbox
                "flex-grow" => { if let Ok(n) = val.to_ascii_lowercase().parse::<f32>() { if n >= 0.0 { flex_grow_spec = Some(n); } } }
                "flex-shrink" => { if let Ok(n) = val.to_ascii_lowercase().parse::<f32>() { if n >= 0.0 { flex_shrink_spec = Some(n); } } }
                "flex-basis" => { flex_basis_spec = parse_size_spec(val).or(flex_basis_spec); }
                "align-items" => {
                    let v = val.to_ascii_lowercase();
                    align_items_spec = Some(match v.as_str() {
                        "stretch" => AlignItems::Stretch,
                        "flex-start" | "start" => AlignItems::FlexStart,
                        "flex-end" | "end" => AlignItems::FlexEnd,
                        "center" => AlignItems::Center,
                        "baseline" => AlignItems::Baseline,
                        _ => AlignItems::Stretch,
                    });
                }
                // Flex shorthand: flex: <grow> <shrink> <basis> | none | auto | initial
                "flex" => {
                    let v = val.trim().to_ascii_lowercase();
                    if v == "none" {
                        flex_grow_spec = Some(0.0);
                        flex_shrink_spec = Some(0.0);
                        flex_basis_spec = Some(SizeSpecified::Auto);
                    } else if v == "auto" {
                        flex_grow_spec = Some(1.0);
                        flex_shrink_spec = Some(1.0);
                        flex_basis_spec = Some(SizeSpecified::Auto);
                    } else if v == "initial" {
                        flex_grow_spec = Some(0.0);
                        flex_shrink_spec = Some(1.0);
                        flex_basis_spec = Some(SizeSpecified::Auto);
                    } else {
                        let parts: Vec<&str> = v.split_whitespace().filter(|p| !p.is_empty()).collect();
                        if !parts.is_empty() {
                            // First part: grow (number)
                            if let Ok(n) = parts[0].parse::<f32>() { if n >= 0.0 { flex_grow_spec = Some(n); } }
                            // Second part: shrink (number)
                            if parts.len() >= 2 {
                                if let Ok(n) = parts[1].parse::<f32>() { if n >= 0.0 { flex_shrink_spec = Some(n); } }
                            }
                            // Third part: basis (size)
                            if parts.len() >= 3 {
                                if let Some(spec) = parse_size_spec(parts[2]) { flex_basis_spec = Some(spec); }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Build computed style with inheritance
        let mut cs = ComputedStyle::default();
        // display: default fallback by tag if still unspecified
        cs.display = display_spec.unwrap_or_else(|| default_display_for_tag(&info.tag));
        cs.position = position_spec.unwrap_or(Position::Static);
        // box model
        if have_margin { cs.margin = margin_spec; }
        if have_padding { cs.padding = padding_spec; }
        if have_border_width { cs.border_width = border_width_spec; }
        if let Some(st) = border_style_spec { cs.border_style = st; }
        if let Some(bc) = border_color_spec { cs.border_color = bc; }
        if let Some(bg) = background_color_spec { cs.background_color = bg; }
        cs.width = width_spec.unwrap_or(SizeSpecified::Auto);
        cs.height = height_spec.unwrap_or(SizeSpecified::Auto);
        // positioned offsets + overflow
        cs.top = top_spec;
        cs.right = right_spec;
        cs.bottom = bottom_spec;
        cs.left = left_spec;
        cs.overflow = overflow_spec.unwrap_or_default();
        // propagate min/max longhands if specified
        cs.min_width = min_width_spec;
        cs.max_width = max_width_spec;
        cs.min_height = min_height_spec;
        cs.max_height = max_height_spec;
        // inherited properties
        if let Some(ps) = parent_style.as_ref() {
            cs.color = ps.color;
            cs.font_size = ps.font_size;
            cs.line_height = ps.line_height;
            cs.font_weight = ps.font_weight;
            cs.font_style = ps.font_style;
            cs.font_family = ps.font_family.clone();
        }
        if let Some(c) = color_spec { cs.color = c; }
        if let Some(fs) = font_size_spec { cs.font_size = fs; }
        if let Some(fw) = font_weight_spec { cs.font_weight = fw; }
        if let Some(fst) = font_style_spec { cs.font_style = fst; }
        if let Some(ff) = font_family_spec { cs.font_family = ff; }
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
        // Flex properties (non-inherited)
        cs.flex_grow = flex_grow_spec.unwrap_or(0.0);
        cs.flex_shrink = flex_shrink_spec.unwrap_or(1.0);
        cs.flex_basis = flex_basis_spec.unwrap_or(SizeSpecified::Auto);
        cs.align_items = align_items_spec.unwrap_or(AlignItems::Stretch);
        // Inherit + override custom properties environment
        cs.custom_properties = var_env;
        cs
    }
}

impl StyleEngine {
    /// Return true if selector uses unsupported features (sibling combinators for now).
    fn selector_has_unsupported(_sel: &ComplexSelector) -> bool {
        // All features used in Phase 8 subset are supported.
        false
    }

    /// After a stylesheet change, rematch only nodes that could be affected based on
    /// the rightmost simple selector indexes. If any universal selectors exist, we
    /// conservatively rematch all nodes. Marks matched nodes (and descendants) dirty.
    fn targeted_rematch_after_stylesheet_update(&mut self) {
        // If there are universal-indexed selectors, everything could match.
        if !self.rule_index.universal.is_empty() {
            // Rematch all and mark all dirty.
            let keys: Vec<NodeKey> = self.nodes.keys().cloned().collect();
            for k in keys {
                self.rematch_node(k, false);
                self.mark_subtree_dirty(k);
            }
            return;
        }
        // Collect candidate nodes from id/class/tag keys in the rule index.
        let mut candidates: HashSet<NodeKey> = HashSet::new();
        for id in self.rule_index.by_id.keys() {
            if let Some(nodes) = self.nodes_by_id.get(id) {
                candidates.extend(nodes.iter().copied());
            }
        }
        for class in self.rule_index.by_class.keys() {
            if let Some(nodes) = self.nodes_by_class.get(class) {
                candidates.extend(nodes.iter().copied());
            }
        }
        for tag in self.rule_index.by_tag.keys() {
            if let Some(nodes) = self.nodes_by_tag.get(tag) {
                candidates.extend(nodes.iter().copied());
            }
        }
        // If still empty but we have some rules, a safe fallback is to rematch all.
        if candidates.is_empty() && (!self.stylesheet.rules.is_empty()) {
            let keys: Vec<NodeKey> = self.nodes.keys().cloned().collect();
            for k in keys {
                self.rematch_node(k, false);
                self.mark_subtree_dirty(k);
            }
            return;
        }
        for k in candidates {
            self.rematch_node(k, false);
            self.mark_subtree_dirty(k);
        }
    }

    /// Parse a CSS color from a string into a ColorRGBA.
    fn parse_color(input: &str) -> Option<ColorRGBA> {
        let parsed: CssColor = input.parse().ok()?;
        let rgba = parsed.to_rgba8();
        Some(ColorRGBA {
            red: rgba[0].clone(),
            green: rgba[1].clone(),
            blue: rgba[2].clone(),
            alpha: rgba[3].clone(),
        })
    }

    fn rebuild_rule_index(&mut self) {
        let mut map = RuleMap::new();
        index_rules(&self.stylesheet, &mut map);
        self.rule_index = map;
    }

    #[allow(dead_code)]
    fn rematch_all_nodes(&mut self) {
        let keys: Vec<NodeKey> = self.nodes.keys().cloned().collect();
        for k in keys {
            self.rematch_node(k, false);
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

    fn rematch_node(&mut self, node: NodeKey, force: bool) {
        // If not forced and matches for this node are up-to-date for the current rules epoch, skip.
        if !force {
            if let Some(ep) = self.node_match_epoch.get(&node) {
                if *ep == self.rules_epoch {
                    return;
                }
            }
        }
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
            {
                if Self::selector_has_unsupported(sel) {
                    let key = (rr.rule_idx, rr.selector_idx);
                    if !self.warned_selectors.contains(&key) {
                        warn!(
                            "Unsupported selector feature encountered (siblings) in rule {} selector {} — selector will not match until implemented",
                            rr.rule_idx, rr.selector_idx
                        );
                        self.warned_selectors.insert(key);
                    }
                }
                if self.match_complex_selector(node, sel) {
                    matched.push(rr);
                }
            }
        }
        self.matches.insert(node, matched);
        // Mark matches up-to-date for current rules epoch
        self.node_match_epoch.insert(node, self.rules_epoch);
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
                Combinator::NextSibling => {
                    // E + F, we are at F (current). Find immediate previous element sibling and match E (comp)
                    let Some(parent) = self.nodes.get(&current).and_then(|ni| ni.parent) else {
                        return false;
                    };
                    let Some(pi) = self.nodes.get(&parent) else {
                        return false;
                    };
                    // find index of current in parent's children
                    let mut found_match = false;
                    if let Some(pos) = pi.children.iter().position(|k| *k == current) {
                        if pos > 0 {
                            let prev = pi.children[pos - 1];
                            if self.match_compound(prev, comp) {
                                current = prev;
                                found_match = true;
                            }
                        }
                    }
                    if !found_match {
                        return false;
                    }
                }
                Combinator::SubsequentSibling => {
                    // E ~ F, we are at F. Any previous sibling matching E is OK.
                    let Some(parent) = self.nodes.get(&current).and_then(|ni| ni.parent) else {
                        return false;
                    };
                    let Some(pi) = self.nodes.get(&parent) else {
                        return false;
                    };
                    let mut matched_prev = None;
                    if let Some(pos) = pi.children.iter().position(|k| *k == current) {
                        for i in (0..pos).rev() {
                            let sib = pi.children[i];
                            if self.match_compound(sib, comp) {
                                matched_prev = Some(sib);
                                break;
                            }
                        }
                    }
                    if let Some(m) = matched_prev {
                        current = m;
                    } else {
                        return false;
                    }
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
                SimpleSelector::Attribute { name, op_value } => {
                    let key = name.to_ascii_lowercase();
                    match op_value {
                        None => {
                            if !info.attributes.contains_key(&key) {
                                return false;
                            }
                        }
                        Some((op, val)) => {
                            if op != "=" {
                                return false;
                            }
                            match info.attributes.get(&key) {
                                Some(v) if v == val => {}
                                _ => return false,
                            }
                        }
                    }
                }
                SimpleSelector::PseudoClass(pc) => match pc {
                    PseudoClass::Root => {
                        if info.parent.is_some() {
                            return false;
                        }
                    }
                    PseudoClass::FirstChild => {
                        if let Some(parent) = info.parent {
                            if let Some(pi) = self.nodes.get(&parent) {
                                if let Some(first) = pi.children.first() {
                                    if *first != node {
                                        return false;
                                    }
                                } else {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    PseudoClass::LastChild => {
                        if let Some(parent) = info.parent {
                            if let Some(pi) = self.nodes.get(&parent) {
                                if let Some(last) = pi.children.last() {
                                    if *last != node {
                                        return false;
                                    }
                                } else {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                },
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
            if self.computed.remove(&node).is_some() {
                self.style_changed = true;
            }
            self.inline_decls.remove(&node);
            // Remove from dirty set if present
            self.dirty_nodes.remove(&node);
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

impl StyleEngine {
    /// Resolve CSS var() references in a value string using the provided environment.
    /// - Replaces var(--name[, fallback]) with the resolved value.
    /// - Uses fallback only when the variable is undefined.
    /// - Detects simple cycles via a visited stack and aborts to None on cycles or excessive depth.
    fn resolve_vars(input: &str, env: &HashMap<String, String>) -> Option<String> {
        fn find_matching_paren(chars: &[char], mut j: usize) -> Option<usize> {
            let mut depth: i32 = 1;
            while j < chars.len() {
                let c = chars[j];
                if c == '(' {
                    depth += 1;
                } else if c == ')' {
                    depth -= 1;
                    if depth == 0 { return Some(j); }
                }
                j += 1;
            }
            None
        }
        fn split_name_and_fallback(chars: &[char]) -> (String, Option<String>) {
            // Split on first top-level comma
            let mut depth: i32 = 0;
            let mut comma_idx: Option<usize> = None;
            for (idx, &c) in chars.iter().enumerate() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    ',' if depth == 0 => { comma_idx = Some(idx); break; }
                    _ => {}
                }
            }
            if let Some(ci) = comma_idx {
                let name: String = chars[..ci].iter().collect::<String>().trim().to_string();
                let fallback: String = chars[ci+1..].iter().collect::<String>().trim().to_string();
                (name, Some(fallback))
            } else {
                let name: String = chars.iter().collect::<String>().trim().to_string();
                (name, None)
            }
        }
        fn resolve_recursive(value: &str, env: &HashMap<String, String>, stack: &mut Vec<String>, depth: usize) -> Option<String> {
            if depth > 32 { return None; }
            let mut out = String::new();
            let mut i: usize = 0;
            let chars: Vec<char> = value.chars().collect();
            while i < chars.len() {
                // Detect var(
                if i + 3 < chars.len() && chars[i] == 'v' && chars[i+1] == 'a' && chars[i+2] == 'r' && chars[i+3] == '(' {
                    let start_call = i + 4; // position after 'var('
                    // find matching ')'
                    if let Some(end_paren) = find_matching_paren(&chars, start_call) {
                        // extract inside contents
                        let inner: Vec<char> = chars[start_call..end_paren].to_vec();
                        let (name_raw, fallback_raw) = split_name_and_fallback(&inner);
                        let name = name_raw.trim().to_string();
                        // require custom property syntax
                        if !(name.starts_with("--")) {
                            return None;
                        }
                        if let Some(val_raw) = env.get(&name) {
                            // Cycle detection
                            if stack.iter().any(|s| s == &name) { return None; }
                            stack.push(name.clone());
                            let resolved = resolve_recursive(val_raw, env, stack, depth + 1)?;
                            stack.pop();
                            out.push_str(&resolved);
                        } else if let Some(fallback_str) = fallback_raw {
                            let resolved_fb = resolve_recursive(&fallback_str, env, stack, depth + 1)?;
                            out.push_str(&resolved_fb);
                        } else {
                            return None; // undefined and no fallback
                        }
                        i = end_paren + 1; // continue after this var()
                        continue;
                    } else {
                        return None; // malformed var( without closing ')'
                    }
                }
                out.push(chars[i]);
                i += 1;
            }
            Some(out)
        }
        resolve_recursive(input, env, &mut Vec::new(), 0)
    }
}
