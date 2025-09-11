use anyhow::Error;
use js::{DOMUpdate, DOMSubscriber, DOMMirror, NodeKey};
use log::{debug, trace, warn};
use std::collections::{HashMap, VecDeque, HashSet};
use std::time::Instant;
use css::types::Stylesheet;
use style_engine::ComputedStyle;

pub mod layout;
mod printing;

pub use layout::LayoutRect;

#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    Document,
    Block { tag: String },
    InlineText { text: String },
}

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub kind: LayoutNodeKind,
    pub attrs: HashMap<String, String>,
    pub parent: Option<NodeKey>,
    pub children: Vec<NodeKey>,
}

impl LayoutNode {
    fn new_document() -> Self {
        Self { kind: LayoutNodeKind::Document, attrs: HashMap::new(), parent: None, children: Vec::new() }
    }
    fn new_block(tag: String, parent: Option<NodeKey>) -> Self {
        Self { kind: LayoutNodeKind::Block { tag }, attrs: HashMap::new(), parent, children: Vec::new() }
    }
    fn new_text(text: String, parent: Option<NodeKey>) -> Self {
        Self { kind: LayoutNodeKind::InlineText { text }, attrs: HashMap::new(), parent, children: Vec::new() }
    }
}

/// Layouter mirrors the DOM and computes layout geometry.
/// It now tracks dirtiness for incremental layout groundwork.
pub struct Layouter {
    nodes: HashMap<NodeKey, LayoutNode>,
    root: NodeKey,
    stylesheet: Stylesheet,
    computed: HashMap<NodeKey, ComputedStyle>,
    /// Global flag indicating that some change requires a layout recompute.
    layout_dirty: bool,
    /// Monotonic epoch incremented on each change affecting layout.
    last_change_epoch: u64,
    /// Per-node dirty flags used for incremental reflow (groundwork only).
    dirty_map: HashMap<NodeKey, DirtyKind>,
    /// A queue of dirty roots to schedule incremental reflow in a stable order.
    dirty_roots_queue: VecDeque<NodeKey>,
    /// Set for O(1) containment checks for the dirty roots queue.
    dirty_root_set: HashSet<NodeKey>,
    /// Cached per-node layout geometry for incremental reflow.
    cached_layout: HashMap<NodeKey, LayoutRect>,
    /// Cached ancestor constraints per node: (inline_available, block_available).
    constraints_cache: HashMap<NodeKey, (i32, i32)>,
    /// Dirty rectangles produced by the last layout computation (for renderer integration).
    dirty_rects: Vec<LayoutRect>,
    /// Telemetry: total number of DOM updates applied to the layouter mirror.
    perf_updates_applied: u64,
    /// Telemetry: number of nodes reflowed in the last compute.
    perf_nodes_reflowed_last: u64,
    /// Telemetry: cumulative nodes reflowed across computes.
    perf_nodes_reflowed_total: u64,
    /// Telemetry: number of dirty subtrees processed in the last compute.
    perf_dirty_subtrees_last: u64,
    /// Telemetry: last layout time in milliseconds.
    perf_layout_time_last_ms: u64,
    /// Telemetry: cumulative layout time in milliseconds.
    perf_layout_time_total_ms: u64,
}

/// Kinds of dirtiness and metadata flags that can affect layout and paint.
/// Multiple flags can be combined. For backward-compatibility, GEOMETRY and
/// LAYOUT share the same bit.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DirtyKind(u32);

impl DirtyKind {
    /// No dirtiness.
    pub const NONE: DirtyKind = DirtyKind(0);
    /// Structural changes: insertion/removal/reparenting.
    pub const STRUCTURE: DirtyKind = DirtyKind(1 << 0);
    /// Style changes: attributes/computed styles affecting layout.
    pub const STYLE: DirtyKind = DirtyKind(1 << 1);
    /// Layout/geometry changes: size/position potentially altered.
    pub const LAYOUT: DirtyKind = DirtyKind(1 << 2);
    /// Alias for legacy callers; same bit as LAYOUT.
    pub const GEOMETRY: DirtyKind = DirtyKind(1 << 2);
    /// Paint-only changes: color/decoration changes that do not affect layout.
    pub const PAINT: DirtyKind = DirtyKind(1 << 3);

    // Axis qualifiers (optional metadata)
    pub const INLINE_AXIS: DirtyKind = DirtyKind(1 << 4);
    pub const BLOCK_AXIS: DirtyKind = DirtyKind(1 << 5);

    // Reason qualifiers (optional metadata)
    pub const REASON_TEXT: DirtyKind = DirtyKind(1 << 6);
    pub const REASON_ATTR: DirtyKind = DirtyKind(1 << 7);
    pub const REASON_STYLE: DirtyKind = DirtyKind(1 << 8);
    pub const REASON_STRUCTURE: DirtyKind = DirtyKind(1 << 9);

    /// Combine two dirty kinds.
    pub fn or(self, other: DirtyKind) -> DirtyKind { DirtyKind(self.0 | other.0) }
    /// Check if all flags in `other` are present.
    pub fn contains(self, other: DirtyKind) -> bool { (self.0 & other.0) == other.0 }
}

impl Layouter {
    /// Create a new Layouter with an empty tree seeded with the Document root.
    pub fn new() -> Self {
        let mut nodes = HashMap::new();
        // Seed root node
        nodes.insert(NodeKey::ROOT, LayoutNode::new_document());
        Self {
            nodes,
            root: NodeKey::ROOT,
            stylesheet: Stylesheet::default(),
            computed: HashMap::new(),
            layout_dirty: false,
            last_change_epoch: 0,
            dirty_map: HashMap::new(),
            dirty_roots_queue: VecDeque::new(),
            dirty_root_set: HashSet::new(),
            cached_layout: HashMap::new(),
            constraints_cache: HashMap::new(),
            dirty_rects: Vec::new(),
            perf_updates_applied: 0,
            perf_nodes_reflowed_last: 0,
            perf_nodes_reflowed_total: 0,
            perf_dirty_subtrees_last: 0,
            perf_layout_time_last_ms: 0,
            perf_layout_time_total_ms: 0,
        }
    }

    pub fn root(&self) -> NodeKey { self.root }

    /// Replace the current computed styles snapshot (from StyleEngine).
    /// This does not automatically mark dirty; HtmlPage decides when styles changed.
    pub fn set_computed_styles(&mut self, map: HashMap<NodeKey, ComputedStyle>) {
        self.computed = map;
    }

    /// Mark a node as dirty with the provided kind(s) and set the global layout flag.
    pub fn mark_dirty(&mut self, node: NodeKey, kind: DirtyKind) {
        let entry = self.dirty_map.entry(node).or_insert(DirtyKind::NONE);
        *entry = entry.or(kind);
        // Enqueue as a dirty root candidate for STRUCTURE or STYLE changes
        if kind.contains(DirtyKind::STRUCTURE) || kind.contains(DirtyKind::STYLE) {
            self.enqueue_dirty_root_candidate(node);
        }
        self.layout_dirty = true;
        self.last_change_epoch = self.last_change_epoch.wrapping_add(1);
    }

    /// Enqueue a node as a dirty root candidate (deduplicated). This queue will be
    /// compacted into minimal roots before reflow.
    fn enqueue_dirty_root_candidate(&mut self, node: NodeKey) {
        if self.dirty_root_set.contains(&node) { return; }
        self.dirty_roots_queue.push_back(node);
        self.dirty_root_set.insert(node);
    }

    /// Rebuild the dirty roots queue from the current dirty_map to ensure minimal
    /// roots (exclude nodes whose parent is also dirty in STRUCTURE or STYLE).
    fn rebuild_dirty_roots_queue(&mut self) {
        self.dirty_roots_queue.clear();
        self.dirty_root_set.clear();
        for r in self.dirty_roots() {
            self.dirty_roots_queue.push_back(r);
            self.dirty_root_set.insert(r);
        }
    }

    /// Mark all ancestors of the given node (up to the root) as dirty with the provided kind(s).
    pub fn mark_ancestors_dirty(&mut self, mut node: NodeKey, kind: DirtyKind) {
        // Walk parent chain; if node is missing, stop.
        while let Some(parent_key) = self.nodes.get(&node).and_then(|n| n.parent) {
            self.mark_dirty(parent_key, kind);
            if parent_key == NodeKey::ROOT { break; }
            node = parent_key;
        }
    }

    /// Atomically read and clear the global layout dirty flag.
    pub fn take_and_clear_layout_dirty(&mut self) -> bool {
        let was_dirty = self.layout_dirty;
        self.layout_dirty = false;
        was_dirty
    }

    /// Read-only access to computed styles.
    pub fn computed_styles(&self) -> &HashMap<NodeKey, ComputedStyle> { &self.computed }

    /// Mark multiple nodes as style-dirty and mark ancestors geometry-dirty (used when StyleEngine reports changes).
    pub fn mark_nodes_style_dirty(&mut self, nodes: &[NodeKey]) {
        for &node in nodes {
            // Mark node style/layout dirty on both axes; record reason for observability
            let node_flags = DirtyKind::STYLE
                .or(DirtyKind::LAYOUT)
                .or(DirtyKind::INLINE_AXIS)
                .or(DirtyKind::BLOCK_AXIS)
                .or(DirtyKind::REASON_STYLE);
            self.mark_dirty(node, node_flags);
            // Ancestors typically need block-axis reflow
            let ancestor_flags = DirtyKind::LAYOUT
                .or(DirtyKind::BLOCK_AXIS)
                .or(DirtyKind::REASON_STYLE);
            self.mark_ancestors_dirty(node, ancestor_flags);
        }
    }

    /// Return a cloned map of attributes per node (for layout/style resolution).
    pub fn attrs_map(&self) -> HashMap<NodeKey, HashMap<String, String>> {
        let mut out = HashMap::new();
        for (k, n) in self.nodes.iter() {
            out.insert(*k, n.attrs.clone());
        }
        out
    }

    /// Drain and return dirty rectangles computed by the last layout pass.
    pub fn take_dirty_rects(&mut self) -> Vec<LayoutRect> {
        let mut out = Vec::new();
        std::mem::swap(&mut out, &mut self.dirty_rects);
        out
    }

    /// Return the current dirty kind flags for a node (for testing/inspection).
    pub fn dirty_kind_of(&self, node: NodeKey) -> DirtyKind {
        *self.dirty_map.get(&node).unwrap_or(&DirtyKind::NONE)
    }

    /// Performance counter: total DOM updates applied to the layouter mirror.
    pub fn perf_updates_applied(&self) -> u64 { self.perf_updates_applied }
    /// Performance counter: nodes reflowed in the last layout compute.
    pub fn perf_nodes_reflowed_last(&self) -> u64 { self.perf_nodes_reflowed_last }
    /// Performance counter: cumulative nodes reflowed across computes.
    pub fn perf_nodes_reflowed_total(&self) -> u64 { self.perf_nodes_reflowed_total }
    /// Performance counter: number of dirty subtrees processed in the last compute.
    pub fn perf_dirty_subtrees_last(&self) -> u64 { self.perf_dirty_subtrees_last }
    /// Performance metric: time spent in the last layout compute in milliseconds.
    pub fn perf_layout_time_last_ms(&self) -> u64 { self.perf_layout_time_last_ms }
    /// Performance metric: cumulative layout time in milliseconds.
    pub fn perf_layout_time_total_ms(&self) -> u64 { self.perf_layout_time_total_ms }

    /// Internal implementation for applying a single DOM update to the layout tree mirror.
    fn apply_update_impl(&mut self, update: DOMUpdate) -> Result<(), Error> {
        // Telemetry: count every DOM update applied to the layouter mirror
        self.perf_updates_applied = self.perf_updates_applied.saturating_add(1);
        use DOMUpdate::*;
        match update {
            InsertElement { parent, node, tag, pos } => {
                trace!("InsertElement parent={:?} node={:?} tag={} pos={}", parent, node, tag, pos);
                self.ensure_parent_exists(parent);
                {
                    let entry = self
                        .nodes
                        .entry(node)
                        .or_insert_with(|| LayoutNode::new_block(tag.clone(), Some(parent)));
                    entry.kind = LayoutNodeKind::Block { tag };
                    entry.parent = Some(parent);
                }
                let parent_children = &mut self
                    .nodes
                    .get_mut(&parent)
                    .expect("parent must exist")
                    .children;
                if pos >= parent_children.len() {
                    parent_children.push(node);
                } else {
                    parent_children.insert(pos, node);
                }
                // Invalidate: element structure affects block-axis layout
                let node_flags = DirtyKind::STRUCTURE
                    .or(DirtyKind::LAYOUT)
                    .or(DirtyKind::BLOCK_AXIS)
                    .or(DirtyKind::REASON_STRUCTURE);
                self.mark_dirty(node, node_flags);
                let parent_flags = DirtyKind::LAYOUT
                    .or(DirtyKind::BLOCK_AXIS)
                    .or(DirtyKind::REASON_STRUCTURE);
                self.mark_dirty(parent, parent_flags);
                self.mark_ancestors_dirty(parent, parent_flags);
            }
            InsertText { parent, node, text, pos } => {
                trace!("InsertText parent={:?} node={:?} text='{}' pos={}", parent, node, text.replace("\n", "\\n"), pos);
                self.ensure_parent_exists(parent);
                {
                    let entry = self
                        .nodes
                        .entry(node)
                        .or_insert_with(|| LayoutNode::new_text(text.clone(), Some(parent)));
                    entry.kind = LayoutNodeKind::InlineText { text };
                    entry.parent = Some(parent);
                }
                let parent_children = &mut self
                    .nodes
                    .get_mut(&parent)
                    .expect("parent must exist")
                    .children;
                if pos >= parent_children.len() {
                    parent_children.push(node);
                } else {
                    parent_children.insert(pos, node);
                }
                // Invalidate: text affects inline-axis layout
                let node_flags = DirtyKind::STRUCTURE
                    .or(DirtyKind::LAYOUT)
                    .or(DirtyKind::INLINE_AXIS)
                    .or(DirtyKind::REASON_TEXT);
                self.mark_dirty(node, node_flags);
                let parent_flags = DirtyKind::LAYOUT
                    .or(DirtyKind::INLINE_AXIS)
                    .or(DirtyKind::REASON_TEXT);
                self.mark_dirty(parent, parent_flags);
                self.mark_ancestors_dirty(parent, parent_flags);
            }
            SetAttr { node, name, value } => {
                trace!("SetAttr node={:?} {}='{}'", node, name, value);
                let entry = self.nodes.entry(node).or_insert_with(LayoutNode::new_document);
                entry.attrs.insert(name, value);
                // Conservative: any attribute may affect style; mark paint and potential layout
                let node_flags = DirtyKind::STYLE
                    .or(DirtyKind::PAINT)
                    .or(DirtyKind::LAYOUT)
                    .or(DirtyKind::REASON_ATTR);
                self.mark_dirty(node, node_flags);
                let ancestor_flags = DirtyKind::LAYOUT.or(DirtyKind::REASON_ATTR);
                self.mark_ancestors_dirty(node, ancestor_flags);
            }
            RemoveNode { node } => {
                trace!("RemoveNode node={:?}", node);
                let parent = self.nodes.get(&node).and_then(|n| n.parent);
                self.remove_node_recursive(node);
                if let Some(p) = parent {
                    let parent_flags = DirtyKind::STRUCTURE
                        .or(DirtyKind::LAYOUT)
                        .or(DirtyKind::BLOCK_AXIS)
                        .or(DirtyKind::REASON_STRUCTURE);
                    self.mark_dirty(p, parent_flags);
                    self.mark_ancestors_dirty(p, parent_flags);
                } else {
                    // If we do not know the parent, mark root as layout-dirty as a fallback
                    self.mark_dirty(NodeKey::ROOT, DirtyKind::LAYOUT.or(DirtyKind::REASON_STRUCTURE));
                }
            }
            EndOfDocument => {
                debug!("EndOfDocument received by layouter");
            }
        }
        Ok(())
    }

    /// Apply a single DOM update to the layout tree mirror (public API).
    pub fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        self.apply_update_impl(update)
    }

    /// Apply a batch of updates.
    pub fn apply_updates<I: IntoIterator<Item = DOMUpdate>>(&mut self, updates: I) -> Result<(), Error> {
        for u in updates { self.apply_update(u)?; }
        Ok(())
    }

    /// Update the active stylesheet used for layout/style resolution (placeholder for now).
    pub fn set_stylesheet(&mut self, stylesheet: Stylesheet) {
        self.stylesheet = stylesheet;
    }

    pub fn stylesheet(&self) -> &Stylesheet { &self.stylesheet }

    /// Compute layout using the dedicated layout module.
    /// Compute layout using either incremental reflow or full layout as a fallback.
    /// Returns the number of nodes (boxes) processed. This may be the count of
    /// reflowed nodes in incremental mode or total laid-out nodes in full mode.
    pub fn compute_layout(&mut self) -> usize {
        // Fallback threshold: if too many dirty roots, do full layout
        let fallback_threshold: f32 = 0.3;
        self.compute_layout_incremental(fallback_threshold)
    }

    /// Force a full layout pass. Intended for benchmarks and diagnostics.
    /// Returns the number of boxes processed across the entire tree.
    pub fn compute_layout_full_for_bench(&mut self) -> usize {
        self.compute_layout_full()
    }

    /// Force an incremental layout pass without fallback to full.
    /// If no cached layout is available yet, this will perform a full pass internally.
    /// Returns the number of nodes reflowed in the incremental pass.
    pub fn compute_layout_incremental_for_bench(&mut self) -> usize {
        // Use a threshold above 1.0 to disable fallback due to dirty-root ratio.
        self.compute_layout_incremental(1.1)
    }

    /// Compute per-node layout geometry (x, y, width, height) for the current tree.
    pub fn compute_layout_geometry(&self) -> HashMap<NodeKey, LayoutRect> {
        layout::compute_layout_geometry(self)
    }

    /// Get a snapshot of the current layout tree for debugging/inspection.
    pub fn snapshot(&self) -> Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)> {
        let mut v: Vec<_> = self
            .nodes
            .iter()
            .map(|(k, n)| (k.clone(), n.kind.clone(), n.children.clone()))
            .collect();
        v.sort_by_key(|(k, _, _)| k.0);
        v
    }

    fn ensure_parent_exists(&mut self, parent: NodeKey) {
        if !self.nodes.contains_key(&parent) {
            warn!("Parent {:?} missing in layouter; creating placeholder Document child", parent);
            self.nodes.insert(parent, LayoutNode::new_document());
        }
    }

    fn remove_node_recursive(&mut self, node: NodeKey) {
        if let Some(n) = self.nodes.remove(&node) {
            // detach from parent
            if let Some(p) = n.parent {
                if let Some(parent_node) = self.nodes.get_mut(&p) {
                    parent_node.children.retain(|c| *c != node);
                }
            }
            // remove children
            for c in n.children {
                self.remove_node_recursive(c);
            }
        }
    }
}

impl DOMSubscriber for Layouter {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        self.apply_update_impl(update)
    }
}

pub type LayouterMirror = DOMMirror<Layouter>;
impl Layouter {
    /// Enumerate minimal set of dirty roots: nodes that have STRUCTURE or STYLE dirtiness
    /// and whose parent is not dirty in those kinds.
    fn dirty_roots(&self) -> Vec<NodeKey> {
        let mut candidates: Vec<NodeKey> = self
            .dirty_map
            .iter()
            .filter_map(|(k, kind)| if kind.contains(DirtyKind::STRUCTURE) || kind.contains(DirtyKind::STYLE) { Some(*k) } else { None })
            .collect();
        candidates.sort_by_key(|k| k.0);
        let dirty_set: std::collections::HashSet<NodeKey> = candidates.iter().cloned().collect();
        candidates
            .into_iter()
            .filter(|k| {
                let parent = self.nodes.get(k).and_then(|n| n.parent);
                match parent {
                    Some(p) => !dirty_set.contains(&p),
                    None => true,
                }
            })
            .collect()
    }

    /// Clear all per-node dirty flags.
    fn clear_dirty_flags(&mut self) { self.dirty_map.clear(); }

    /// Collect all nodes in the subtree rooted at `node`, including the root.
    fn collect_subtree_keys(&self, node: NodeKey, out: &mut Vec<NodeKey>) {
        out.push(node);
        if let Some(n) = self.nodes.get(&node) {
            for child in &n.children {
                self.collect_subtree_keys(*child, out);
            }
        }
    }

    /// Full layout pass: recompute count and update cached geometry for all nodes.
    fn compute_layout_full(&mut self) -> usize {
        let start = Instant::now();
        // Keep old cache for diffing
        let old_cache = self.cached_layout.clone();
        let count = layout::compute_simple_layout(self);
        let new_rects = layout::compute_layout_geometry(self);
        // Build a simple constraints cache: inherit parent border-box width as available space
        let mut constraints: HashMap<NodeKey, (i32, i32)> = HashMap::new();
        for (k, n) in self.nodes.iter() {
            let parent_width = n.parent.and_then(|p| new_rects.get(&p).map(|r| r.width)).unwrap_or(800);
            constraints.insert(*k, (parent_width, parent_width));
        }
        self.constraints_cache = constraints;
        // Compute dirty rects by comparing old and new per node
        let mut dirty: Vec<LayoutRect> = Vec::new();
        // Changes/removals
        for (k, old_rect) in old_cache.iter() {
            match new_rects.get(k) {
                Some(new_rect) if new_rect != old_rect => {
                    dirty.push(*old_rect);
                    dirty.push(*new_rect);
                }
                None => {
                    dirty.push(*old_rect);
                }
                _ => {}
            }
        }
        // Additions
        for (k, new_rect) in new_rects.iter() {
            if !old_cache.contains_key(k) {
                dirty.push(*new_rect);
            }
        }
        self.cached_layout = new_rects;
        self.clear_dirty_flags();
        self.dirty_rects = dirty;
        // Telemetry
        self.perf_nodes_reflowed_last = self.dirty_rects.len() as u64; // proxy: changed nodes* (approx)
        self.perf_nodes_reflowed_total = self.perf_nodes_reflowed_total.saturating_add(self.perf_nodes_reflowed_last);
        self.perf_dirty_subtrees_last = 1; // full pass treated as one big subtree
        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.perf_layout_time_last_ms = elapsed_ms;
        self.perf_layout_time_total_ms = self.perf_layout_time_total_ms.saturating_add(elapsed_ms);
        count
    }

    /// Incremental layout: attempt to limit work to dirty subtrees; fallback to full if too large.
    fn compute_layout_incremental(&mut self, fallback_threshold: f32) -> usize {
        // If we do not yet have a cache, run a full pass.
        if self.cached_layout.is_empty() {
            return self.compute_layout_full();
        }
        let start = Instant::now();
        let total_nodes = self.nodes.len().saturating_sub(1).max(1);
        if self.dirty_roots_queue.is_empty() { self.rebuild_dirty_roots_queue(); }
        let roots: Vec<NodeKey> = self.dirty_roots_queue.iter().cloned().collect();
        if roots.is_empty() {
            self.dirty_rects.clear();
            self.perf_nodes_reflowed_last = 0;
            self.perf_dirty_subtrees_last = 0;
            self.perf_layout_time_last_ms = 0;
            return 0;
        }
        if (roots.len() as f32) / (total_nodes as f32) >= fallback_threshold {
            return self.compute_layout_full();
        }
        // MVP approach: compute a fresh geometry snapshot for correctness,
        // then selectively update the cache for nodes in dirty subtrees.
        let new_rects = layout::compute_layout_geometry(self);
        // Update constraints cache (approximate) from new rects using parent width
        let mut constraints: HashMap<NodeKey, (i32, i32)> = HashMap::new();
        for (k, n) in self.nodes.iter() {
            let parent_width = n.parent.and_then(|p| new_rects.get(&p).map(|r| r.width)).unwrap_or(800);
            constraints.insert(*k, (parent_width, parent_width));
        }
        self.constraints_cache = constraints;
        let mut reflowed_nodes: usize = 0;
        let mut dirty: Vec<LayoutRect> = Vec::new();
        #[cfg(feature = "parallel_layout")]
        {
            use rayon::prelude::*;
            let subtrees: Vec<Vec<NodeKey>> = roots
                .iter()
                .map(|r| {
                    let mut v = Vec::new();
                    self.collect_subtree_keys(*r, &mut v);
                    v
                })
                .collect();
            let diffs: Vec<(NodeKey, Option<LayoutRect>, Option<LayoutRect>)> = subtrees
                .par_iter()
                .flat_map(|sub| {
                    sub.iter()
                        .map(|k| (*k, self.cached_layout.get(k).cloned(), new_rects.get(k).cloned()))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (k, old, new_opt) in diffs {
                match new_opt {
                    Some(r) => {
                        if old.map(|o| o != r).unwrap_or(true) {
                            reflowed_nodes = reflowed_nodes.saturating_add(1);
                            if let Some(o) = old { dirty.push(o); }
                            dirty.push(r);
                        }
                        self.cached_layout.insert(k, r);
                    }
                    None => {
                        if let Some(o) = old { dirty.push(o); }
                        if self.cached_layout.remove(&k).is_some() {
                            reflowed_nodes = reflowed_nodes.saturating_add(1);
                        }
                    }
                }
            }
        }
        #[cfg(not(feature = "parallel_layout"))]
        {
            for root in &roots {
                let mut subtree: Vec<NodeKey> = Vec::new();
                self.collect_subtree_keys(*root, &mut subtree);
                for k in subtree {
                    let old = self.cached_layout.get(&k).cloned();
                    match new_rects.get(&k) {
                        Some(r) => {
                            let old_rect = old;
                            if old_rect.map(|o| o != *r).unwrap_or(true) {
                                reflowed_nodes = reflowed_nodes.saturating_add(1);
                                if let Some(o) = old_rect { dirty.push(o); }
                                dirty.push(*r);
                            }
                            self.cached_layout.insert(k, *r);
                        }
                        None => {
                            if let Some(o) = old { dirty.push(o); }
                            if self.cached_layout.remove(&k).is_some() {
                                reflowed_nodes = reflowed_nodes.saturating_add(1);
                            }
                        }
                    }
                }
            }
        }
        self.clear_dirty_flags();
        // Clear scheduled roots after processing so future mutations re-enqueue
        self.dirty_roots_queue.clear();
        self.dirty_root_set.clear();
        self.dirty_rects = dirty;
        // Telemetry
        self.perf_nodes_reflowed_last = reflowed_nodes as u64;
        self.perf_nodes_reflowed_total = self.perf_nodes_reflowed_total.saturating_add(self.perf_nodes_reflowed_last);
        self.perf_dirty_subtrees_last = roots.len() as u64;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.perf_layout_time_last_ms = elapsed_ms;
        self.perf_layout_time_total_ms = self.perf_layout_time_total_ms.saturating_add(elapsed_ms);
        reflowed_nodes
    }

    /// Print current dirty state for debugging purposes.
    pub fn print_dirty_state(&self) {
        for (k, kind) in &self.dirty_map {
            log::trace!("Dirty node {:?}: kind={:?}", k, kind);
        }
    }
}
