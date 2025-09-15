use anyhow::Error;
use css::types as css_types;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;
use std::mem::take;
use style_engine as se;

pub mod layout {
    pub fn collapse_whitespace(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut in_ws = false;
        for ch in s.chars() {
            if ch.is_whitespace() {
                if !in_ws {
                    out.push(' ');
                    in_ws = true;
                }
            } else {
                in_ws = false;
                out.push(ch);
            }
        }
        out.trim().to_string()
    }
    pub fn reorder_bidi_for_display(s: &str) -> String {
        s.to_string()
    }
}

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
    computed_styles: HashMap<NodeKey, se::ComputedStyle>,
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
    pub fn new() -> Self {
        Self::default()
    }
    pub fn snapshot(&self) -> Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)> {
        if self.nodes.is_empty() {
            vec![(NodeKey::ROOT, LayoutNodeKind::Document, vec![])]
        } else {
            // Return a shallow snapshot of known nodes without computing children here.
            self.nodes
                .iter()
                .map(|(k, kind)| (*k, kind.clone(), Vec::new()))
                .collect()
        }
    }
    pub fn attrs_map(&self) -> HashMap<NodeKey, HashMap<String, String>> {
        HashMap::new()
    }
    pub fn set_stylesheet(&mut self, s: css_types::Stylesheet) {
        self.stylesheet = s;
    }
    pub fn set_computed_styles(&mut self, map: HashMap<NodeKey, se::ComputedStyle>) {
        self.computed_styles = map;
    }
    pub fn has_material_dirty(&self) -> bool {
        false
    }
    pub fn mark_noop_layout_tick(&mut self) {
        self.perf_nodes_reflowed_last = 0;
        self.perf_dirty_subtrees_last = 0;
        self.perf_layout_time_last_ms = 0;
    }
    pub fn compute_layout(&mut self) -> usize {
        0
    }
    pub fn compute_layout_geometry(&self) -> HashMap<NodeKey, LayoutRect> {
        self.rects.clone()
    }
    pub fn take_dirty_rects(&mut self) -> Vec<LayoutRect> {
        take(&mut self.dirty_rects)
    }
    pub fn perf_nodes_reflowed_last(&self) -> u64 {
        self.perf_nodes_reflowed_last
    }
    pub fn perf_dirty_subtrees_last(&self) -> u64 {
        self.perf_dirty_subtrees_last
    }
    pub fn perf_layout_time_last_ms(&self) -> u64 {
        self.perf_layout_time_last_ms
    }
    pub fn perf_layout_time_total_ms(&self) -> u64 {
        self.perf_layout_time_total_ms
    }
    pub fn perf_line_boxes_last(&self) -> u64 {
        self.perf_line_boxes_last
    }
    pub fn perf_shaped_runs_last(&self) -> u64 {
        self.perf_shaped_runs_last
    }
    pub fn perf_early_outs_last(&self) -> u64 {
        self.perf_early_outs_last
    }
    pub fn perf_updates_applied(&self) -> u64 {
        self.perf_updates_applied
    }

    pub fn hit_test(&mut self, _x: i32, _y: i32) -> Option<NodeKey> {
        None
    }
    pub fn mark_nodes_style_dirty(&mut self, _nodes: &[NodeKey]) { /* no-op shim */
    }
    pub fn computed_styles(&self) -> &HashMap<NodeKey, se::ComputedStyle> {
        &self.computed_styles
    }
}

impl DOMSubscriber for Layouter {
    fn apply_update(&mut self, _update: DOMUpdate) -> Result<(), Error> {
        self.perf_updates_applied = self.perf_updates_applied.saturating_add(1);
        Ok(())
    }
}
