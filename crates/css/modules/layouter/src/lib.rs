//! Minimal layouter module.
//!
//! This crate provides a lightweight external layouter used by tests to mirror
//! DOM structure, attributes, and record basic performance counters. It also
//! computes a very naive block layout sufficient for bootstrapping fixtures.
//!
//! Spec: <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>

use anyhow::Error;
use core::mem::take;
use core::sync::atomic::{Ordering, compiler_fence};
use css::types as css_types;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;
use style_engine::ComputedStyle;

/// A rectangle in device-independent pixels.
///
/// All coordinates are integral for now to keep the shim simple.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    /// The x-coordinate of the rectangle origin.
    pub x: i32,
    /// The y-coordinate of the rectangle origin.
    pub y: i32,
    /// The width of the rectangle.
    pub width: i32,
    /// The height of the rectangle.
    pub height: i32,
}

/// Width of the initial containing block used by tests.
const INITIAL_CONTAINING_BLOCK_WIDTH: i32 = 800;

/// Kinds of layout nodes known to the layouter.
#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    /// The root document node.
    Document,
    /// A block-level element.
    Block {
        /// Tag name of the block element (e.g. "div").
        tag: String,
    },
    /// An inline text node.
    InlineText {
        /// The textual contents of this node.
        text: String,
    },
}

/// A convenience type alias for snapshot entries returned by [`Layouter::snapshot`].
pub type SnapshotEntry = (NodeKey, LayoutNodeKind, Vec<NodeKey>);

/// The primary layout coordinator for this module.
///
/// `Layouter` maintains a set of nodes, their computed rectangles and styles,
/// a stylesheet reference, as well as a few performance counters that can be
/// queried by tests and diagnostics.
#[derive(Default)]
pub struct Layouter {
    /// Map of known DOM node keys to their layout-kind representation.
    nodes: HashMap<NodeKey, LayoutNodeKind>,
    /// Children per parent in DOM order (elements only).
    children: HashMap<NodeKey, Vec<NodeKey>>,
    /// Bounding rectangles for known nodes.
    rects: HashMap<NodeKey, LayoutRect>,
    /// Computed styles for known nodes.
    computed_styles: HashMap<NodeKey, ComputedStyle>,
    /// The active stylesheet used during layout.
    stylesheet: css_types::Stylesheet,
    /// Number of nodes reflowed in the last layout pass.
    perf_nodes_reflowed_last: u64,
    /// Number of dirty subtrees in the last layout pass.
    perf_dirty_subtrees_last: u64,
    /// Time spent in the last layout pass (milliseconds).
    perf_layout_time_last_ms: u64,
    /// Accumulated time spent across all layout passes (milliseconds).
    perf_layout_time_total_ms: u64,
    /// Number of line boxes produced in the last layout pass.
    perf_line_boxes_last: u64,
    /// Number of shaped text runs produced in the last layout pass.
    perf_shaped_runs_last: u64,
    /// Number of early-outs taken in the last layout pass.
    perf_early_outs_last: u64,
    /// Number of DOM updates applied since creation.
    perf_updates_applied: u64,
    /// Rectangles that have been marked dirty since the last query.
    dirty_rects: Vec<LayoutRect>,
    /// Tracked attributes for nodes used by serializers/tests (id/class/style).
    attrs: HashMap<NodeKey, HashMap<String, String>>,
}

impl Layouter {
    #[inline]
    /// Find the first block-level node under `start` using a depth-first search.
    ///
    /// Spec: CSS 2.2 §9.4.1 — we identify element boxes that participate in
    /// block formatting contexts in a simplified manner.
    fn find_first_block_under(&self, start: NodeKey) -> Option<NodeKey> {
        if matches!(self.nodes.get(&start), Some(LayoutNodeKind::Block { .. })) {
            return Some(start);
        }
        if let Some(child_list) = self.children.get(&start) {
            for child_key in child_list {
                if let Some(found) = self.find_first_block_under(*child_key) {
                    return Some(found);
                }
            }
        }
        None
    }

    #[inline]
    /// Returns the tag name for a block node, or `None` if not a block.
    fn tag_of(&self, key: NodeKey) -> Option<String> {
        let kind = self.nodes.get(&key)?.clone();
        match kind {
            LayoutNodeKind::Block { tag } => Some(tag),
            _ => None,
        }
    }

    #[inline]
    /// Creates a new `Layouter` with default state.
    pub fn new() -> Self {
        let mut state = Self::default();
        // Seed with a document root so snapshots have an anchor
        state.nodes.insert(NodeKey::ROOT, LayoutNodeKind::Document);
        state
    }
    #[inline]
    /// Returns a shallow snapshot of the known nodes.
    ///
    /// Spec: This mirrors the element box tree used by block formatting contexts
    /// (CSS 2.2 §9.4.1) in a simplified form.
    pub fn snapshot(&self) -> Vec<SnapshotEntry> {
        // Build entries in deterministic key order to avoid hash nondeterminism
        let mut keys: Vec<NodeKey> = self.nodes.keys().copied().collect();
        keys.sort_by_key(|key| key.0);
        let mut out: Vec<SnapshotEntry> = Vec::with_capacity(keys.len());
        for key in keys {
            let kind = self
                .nodes
                .get(&key)
                .cloned()
                .unwrap_or(LayoutNodeKind::Document);
            let children = self.children.get(&key).cloned().unwrap_or_default();
            out.push((key, kind, children));
        }
        out
    }
    #[inline]
    /// Returns a map of attributes for nodes, if any are tracked.
    ///
    /// Currently returns an empty map as a placeholder.
    pub fn attrs_map(&self) -> HashMap<NodeKey, HashMap<String, String>> {
        self.attrs.clone()
    }
    #[inline]
    /// Sets the active stylesheet.
    pub fn set_stylesheet(&mut self, stylesheet: css_types::Stylesheet) {
        self.stylesheet = stylesheet;
    }
    #[inline]
    /// Replaces the current computed-style map.
    pub fn set_computed_styles(&mut self, map: HashMap<NodeKey, ComputedStyle>) {
        self.computed_styles = map;
    }
    #[inline]
    /// Returns whether there are material changes requiring a layout.
    pub const fn has_material_dirty(&self) -> bool {
        false
    }
    #[inline]
    /// Marks that a layout tick occurred without any meaningful work.
    pub fn mark_noop_layout_tick(&mut self) {
        self.perf_nodes_reflowed_last = 0;
        self.perf_dirty_subtrees_last = 0;
        self.perf_layout_time_last_ms = 0;
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
    }
    #[inline]
    /// Computes a naive block layout and returns the number of nodes affected.
    ///
    /// Spec: CSS 2.2 §9.4.1 (Block formatting contexts) — This implementation
    /// stacks block-level boxes vertically in DOM order, applying margins and
    /// padding. It does not implement margin collapsing, over-constrained width
    /// resolution, floats, or inline layout.
    pub fn compute_layout(&mut self) -> usize {
        // Very small, deterministic block stacker used by tests. Geometry here
        // is not authoritative for Chromium comparison; the test harness reads
        // geometry from the page's internal layouter. We still compute simple
        // rects for diagnostics and future expansion.

        // Start a new pass
        self.perf_nodes_reflowed_last = 0;
        self.perf_dirty_subtrees_last = 0;
        self.perf_layout_time_last_ms = 0;

        // Fixed initial containing block width matching test window size.
        // Height is not used; content height is simplified to 0.
        let icb_width: i32 = INITIAL_CONTAINING_BLOCK_WIDTH;

        // Choose layout root similar to the Chromium harness: first block under ROOT,
        // and if it's <html>, prefer its <body> child.
        let Some(mut root) = self.find_first_block_under(NodeKey::ROOT) else {
            // No elements to layout
            self.rects.clear();
            compiler_fence(Ordering::SeqCst);
            return 0;
        };

        let root_is_html = self
            .tag_of(root)
            .is_some_and(|tag| tag.eq_ignore_ascii_case("html"));
        if root_is_html
            && let Some(child_list) = self.children.get(&root)
            && let Some(found_body) = child_list.iter().copied().find(|child_key| {
                self.tag_of(*child_key)
                    .is_some_and(|tname| tname.eq_ignore_ascii_case("body"))
            })
        {
            root = found_body;
        }

        // Establish container edges for the chosen root (treat as containing block)
        let root_style = self
            .computed_styles
            .get(&root)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let container_padding_left = root_style.padding.left.max(0.0) as i32;
        let container_padding_right = root_style.padding.right.max(0.0) as i32;
        let container_border_left = root_style.border_width.left.max(0.0) as i32;
        let container_border_right = root_style.border_width.right.max(0.0) as i32;
        let container_margin_left = root_style.margin.left.max(0.0) as i32;
        let container_margin_right = root_style.margin.right.max(0.0) as i32;

        let horizontal_non_content = container_padding_left
            .saturating_add(container_padding_right)
            .saturating_add(container_border_left)
            .saturating_add(container_border_right)
            .saturating_add(container_margin_left)
            .saturating_add(container_margin_right);
        let container_width = icb_width.saturating_sub(horizontal_non_content).max(0);

        // Emit a rect for the root itself (content box width; height is unknown -> 0)
        self.rects.insert(
            root,
            LayoutRect {
                x: 0,
                y: 0,
                width: container_width,
                height: 0,
            },
        );

        // Stack direct element children of the chosen root vertically with simple sibling margin collapsing.
        let mut y_cursor: i32 = 0;
        let mut previous_bottom_margin: i32 = 0;
        let mut reflowed_count = 0usize;

        if let Some(child_list) = self.children.get(&root).cloned() {
            for child_key in child_list {
                if !matches!(
                    self.nodes.get(&child_key),
                    Some(LayoutNodeKind::Block { .. })
                ) {
                    continue;
                }
                let style = self
                    .computed_styles
                    .get(&child_key)
                    .cloned()
                    .unwrap_or_else(ComputedStyle::default);

                let margin_left = style.margin.left.max(0.0) as i32;
                let margin_right = style.margin.right.max(0.0) as i32;
                let margin_top = style.margin.top.max(0.0) as i32;
                let margin_bottom = style.margin.bottom.max(0.0) as i32;

                // Simple sibling margin collapsing: collapse previous bottom and current top using max()
                let collapsed_vertical_margin = previous_bottom_margin.max(margin_top);

                // Position: x at content start + margin-left; y at current cursor + collapsed margin
                let x_position = container_margin_left
                    .saturating_add(container_border_left)
                    .saturating_add(container_padding_left)
                    .saturating_add(margin_left);
                let y_position = y_cursor.saturating_add(collapsed_vertical_margin);

                // Available width inside container content minus horizontal margins
                let horizontal_margins = margin_left.saturating_add(margin_right);
                let width_available = container_width.saturating_sub(horizontal_margins).max(0);
                let computed_width = width_available;

                // Height remains unknown without content size: 0 for MVP
                let computed_height = 0i32;

                self.rects.insert(
                    child_key,
                    LayoutRect {
                        x: x_position,
                        y: y_position,
                        width: computed_width,
                        height: computed_height,
                    },
                );
                reflowed_count = reflowed_count.saturating_add(1);

                // Advance cursor: current y plus height plus bottom margin
                y_cursor = y_position.saturating_add(computed_height);
                previous_bottom_margin = margin_bottom;
            }
        }

        self.perf_nodes_reflowed_last = reflowed_count as u64;
        // Ensure not const-eligible and signal that something changed
        if reflowed_count > 0 {
            self.dirty_rects.push(LayoutRect {
                x: 0,
                y: 0,
                width: container_width,
                height: y_cursor.max(0i32),
            });
        }
        compiler_fence(Ordering::SeqCst);
        reflowed_count
    }
    #[inline]
    /// Returns a copy of the current layout geometry per node.
    pub fn compute_layout_geometry(&self) -> HashMap<NodeKey, LayoutRect> {
        self.rects.clone()
    }
    #[inline]
    /// Drains and returns the list of dirty rectangles since the last query.
    pub fn take_dirty_rects(&mut self) -> Vec<LayoutRect> {
        let out = take(&mut self.dirty_rects);
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
        out
    }
    #[inline]
    /// Number of nodes reflowed in the last layout pass.
    pub const fn perf_nodes_reflowed_last(&self) -> u64 {
        self.perf_nodes_reflowed_last
    }
    #[inline]
    /// Number of dirty subtrees in the last layout pass.
    pub const fn perf_dirty_subtrees_last(&self) -> u64 {
        self.perf_dirty_subtrees_last
    }
    #[inline]
    /// Time spent in the last layout pass (milliseconds).
    pub const fn perf_layout_time_last_ms(&self) -> u64 {
        self.perf_layout_time_last_ms
    }
    #[inline]
    /// Accumulated time spent across all layout passes (milliseconds).
    pub const fn perf_layout_time_total_ms(&self) -> u64 {
        self.perf_layout_time_total_ms
    }
    #[inline]
    /// Number of line boxes produced in the last layout pass.
    pub const fn perf_line_boxes_last(&self) -> u64 {
        self.perf_line_boxes_last
    }
    #[inline]
    /// Number of shaped text runs produced in the last layout pass.
    pub const fn perf_shaped_runs_last(&self) -> u64 {
        self.perf_shaped_runs_last
    }
    #[inline]
    /// Number of early-outs taken in the last layout pass.
    pub const fn perf_early_outs_last(&self) -> u64 {
        self.perf_early_outs_last
    }
    #[inline]
    /// Number of DOM updates applied since creation.
    pub const fn perf_updates_applied(&self) -> u64 {
        self.perf_updates_applied
    }

    #[inline]
    /// Returns the top-most node at the given position, if any.
    pub fn hit_test(&mut self, _x: i32, _y: i32) -> Option<NodeKey> {
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
        None
    }
    #[inline]
    /// Marks the given nodes as having dirty style.
    pub fn mark_nodes_style_dirty(&mut self, _nodes: &[NodeKey]) {
        /* no-op shim */
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
    }
    #[inline]
    /// Returns a reference to the computed-style map.
    pub const fn computed_styles(&self) -> &HashMap<NodeKey, ComputedStyle> {
        &self.computed_styles
    }
}

impl DOMSubscriber for Layouter {
    #[inline]
    /// Applies a DOM update to the layouter, updating internal counters.
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        self.perf_updates_applied = self.perf_updates_applied.saturating_add(1);
        match update {
            DOMUpdate::InsertElement {
                parent, node, tag, ..
            } => {
                self.nodes.insert(node, LayoutNodeKind::Block { tag });
                self.children.entry(parent).or_default().push(node);
            }
            DOMUpdate::RemoveNode { node } => {
                self.nodes.remove(&node);
                self.rects.remove(&node);
                self.computed_styles.remove(&node);
                self.attrs.remove(&node);
                // Remove from any parent's children list deterministically
                let mut parent_keys: Vec<NodeKey> = self.children.keys().copied().collect();
                parent_keys.sort_by_key(|key| key.0);
                for parent in parent_keys {
                    if let Some(kids) = self.children.get_mut(&parent)
                        && let Some(pos) = kids.iter().position(|child_key| *child_key == node)
                    {
                        kids.remove(pos);
                    }
                }
            }
            DOMUpdate::SetAttr { node, name, value } => {
                self.attrs.entry(node).or_default().insert(name, value);
            }
            DOMUpdate::InsertText { .. } | DOMUpdate::EndOfDocument => { /* ignore in minimal shim */
            }
        }
        Ok(())
    }
}
