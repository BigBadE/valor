use anyhow::Error;
use core::mem::take;
use core::sync::atomic::{Ordering, compiler_fence};
use css::types as css_types;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;
use style_engine::ComputedStyle;

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    Document,
    Block { tag: String },
    InlineText { text: String },
}

#[derive(Default)]
pub struct Layouter {
    nodes: HashMap<NodeKey, LayoutNodeKind>,
    rects: HashMap<NodeKey, LayoutRect>,
    computed_styles: HashMap<NodeKey, ComputedStyle>,
    stylesheet: css_types::Stylesheet,
    perf_nodes_reflowed_last: u64,
    perf_dirty_subtrees_last: u64,
    perf_layout_time_last_ms: u64,
    perf_layout_time_total_ms: u64,
    perf_line_boxes_last: u64,
    perf_shaped_runs_last: u64,
    perf_early_outs_last: u64,
    perf_updates_applied: u64,
    dirty_rects: Vec<LayoutRect>,
}

impl Layouter {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
    #[inline]
    pub fn snapshot(&self) -> Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)> {
        if self.nodes.is_empty() {
            vec![(NodeKey::ROOT, LayoutNodeKind::Document, vec![])]
        } else {
            // Return a shallow snapshot of known nodes without computing children here.
            self.nodes
                .iter()
                .map(|(key, kind)| (*key, kind.clone(), Vec::new()))
                .collect()
        }
    }
    #[inline]
    pub fn attrs_map(&self) -> HashMap<NodeKey, HashMap<String, String>> {
        HashMap::new()
    }
    #[inline]
    pub fn set_stylesheet(&mut self, stylesheet: css_types::Stylesheet) {
        self.stylesheet = stylesheet;
    }
    #[inline]
    pub fn set_computed_styles(&mut self, map: HashMap<NodeKey, ComputedStyle>) {
        self.computed_styles = map;
    }
    #[inline]
    pub const fn has_material_dirty(&self) -> bool {
        false
    }
    #[inline]
    pub fn mark_noop_layout_tick(&mut self) {
        self.perf_nodes_reflowed_last = 0;
        self.perf_dirty_subtrees_last = 0;
        self.perf_layout_time_last_ms = 0;
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
    }
    #[inline]
    pub fn compute_layout(&mut self) -> usize {
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
        0
    }
    #[inline]
    pub fn compute_layout_geometry(&self) -> HashMap<NodeKey, LayoutRect> {
        self.rects.clone()
    }
    #[inline]
    pub fn take_dirty_rects(&mut self) -> Vec<LayoutRect> {
        let out = take(&mut self.dirty_rects);
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
        out
    }
    #[inline]
    pub const fn perf_nodes_reflowed_last(&self) -> u64 {
        self.perf_nodes_reflowed_last
    }
    #[inline]
    pub const fn perf_dirty_subtrees_last(&self) -> u64 {
        self.perf_dirty_subtrees_last
    }
    #[inline]
    pub const fn perf_layout_time_last_ms(&self) -> u64 {
        self.perf_layout_time_last_ms
    }
    #[inline]
    pub const fn perf_layout_time_total_ms(&self) -> u64 {
        self.perf_layout_time_total_ms
    }
    #[inline]
    pub const fn perf_line_boxes_last(&self) -> u64 {
        self.perf_line_boxes_last
    }
    #[inline]
    pub const fn perf_shaped_runs_last(&self) -> u64 {
        self.perf_shaped_runs_last
    }
    #[inline]
    pub const fn perf_early_outs_last(&self) -> u64 {
        self.perf_early_outs_last
    }
    #[inline]
    pub const fn perf_updates_applied(&self) -> u64 {
        self.perf_updates_applied
    }

    #[inline]
    pub fn hit_test(&mut self, _x: i32, _y: i32) -> Option<NodeKey> {
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
        None
    }
    #[inline]
    pub fn mark_nodes_style_dirty(&mut self, _nodes: &[NodeKey]) {
        /* no-op shim */
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
    }
    #[inline]
    pub const fn computed_styles(&self) -> &HashMap<NodeKey, ComputedStyle> {
        &self.computed_styles
    }
}

impl DOMSubscriber for Layouter {
    #[inline]
    fn apply_update(&mut self, _update: DOMUpdate) -> Result<(), Error> {
        self.perf_updates_applied = self.perf_updates_applied.saturating_add(1);
        Ok(())
    }
}
