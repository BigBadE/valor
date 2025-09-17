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
use style_engine::{BoxSizing, ComputedStyle, Position};

/// Metrics for the container box edges and available content width.
#[derive(Clone, Copy, Debug)]
struct ContainerMetrics {
    /// Content box width available to children inside the container.
    container_width: i32,
    /// Container padding-left in pixels (clamped to >= 0).
    padding_left: i32,
    /// Container padding-top in pixels (clamped to >= 0).
    padding_top: i32,
    /// Container border-left width in pixels (clamped to >= 0).
    border_left: i32,
    /// Container border-top width in pixels (clamped to >= 0).
    border_top: i32,
    /// Container margin-left in pixels (may be negative).
    margin_left: i32,
    /// Container margin-top in pixels (may be negative).
    margin_top: i32,
}

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
    /// Collapse two vertical margins `margin_a` and `margin_b` following CSS 2.2 §8.3 rules for pairs:
    /// - If both are positive, result is max(a, b).
    /// - If both are negative, result is min(a, b) (more negative).
    /// - If mixed signs, result is a + b (the algebraic sum of the most positive and most negative values).
    fn collapse_margins_pair(margin_a: i32, margin_b: i32) -> i32 {
        if margin_a >= 0i32 && margin_b >= 0i32 {
            return margin_a.max(margin_b);
        }
        if margin_a <= 0i32 && margin_b <= 0i32 {
            return margin_a.min(margin_b);
        }
        margin_a.saturating_add(margin_b)
    }
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
    /// Computes a naive block layout and returns the number of nodes affected.
    pub fn compute_layout(&mut self) -> usize {
        // Reset perf counters at the top of the pass
        self.perf_layout_time_last_ms = 0;
        self.perf_updates_applied = 0;
        self.perf_nodes_reflowed_last = 0;
        self.perf_dirty_subtrees_last = 0;
        self.perf_layout_time_last_ms = 0;
        // ensure not const-eligible
        compiler_fence(Ordering::SeqCst);
        self.layout_root()
    }

    #[inline]
    /// Compute container metrics for `root` given an initial containing block width.
    fn compute_container_metrics(&self, root: NodeKey, icb_width: i32) -> ContainerMetrics {
        let root_style = self
            .computed_styles
            .get(&root)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);

        let padding_left = root_style.padding.left.max(0.0f32) as i32;
        let padding_right = root_style.padding.right.max(0.0f32) as i32;
        let padding_top = root_style.padding.top.max(0.0f32) as i32;
        let border_left = root_style.border_width.left.max(0.0f32) as i32;
        let border_right = root_style.border_width.right.max(0.0f32) as i32;
        let border_top = root_style.border_width.top.max(0.0f32) as i32;
        let margin_left = root_style.margin.left as i32;
        let margin_right = root_style.margin.right as i32;
        let margin_top = root_style.margin.top as i32;

        let horizontal_non_content = padding_left
            .saturating_add(padding_right)
            .saturating_add(border_left)
            .saturating_add(border_right)
            .saturating_add(margin_left)
            .saturating_add(margin_right);
        let container_width = icb_width.saturating_sub(horizontal_non_content).max(0i32);

        ContainerMetrics {
            container_width,
            padding_left,
            padding_top,
            border_left,
            border_top,
            margin_left,
            margin_top,
        }
    }

    /// Lays out the root node and its children (partial CSS 2.2 support: width/min/max, box-sizing, simple margin collapsing, relative offsets).
    fn layout_root(&mut self) -> usize {
        // Fixed initial containing block width matching test window size.
        // Height is not used; content height is simplified to 0.
        let icb_width: i32 = INITIAL_CONTAINING_BLOCK_WIDTH;

        let Some(root) = self.choose_layout_root() else {
            self.rects.clear();
            compiler_fence(Ordering::SeqCst);
            return 0;
        };

        let metrics = Self::compute_container_metrics(self, root, icb_width);

        // Emit a rect for the root itself (content box width; height is unknown -> 0)
        self.rects.insert(
            root,
            LayoutRect {
                x: 0,
                y: 0,
                width: metrics.container_width,
                height: 0,
            },
        );

        let (reflowed_count, content_height) = self.layout_block_children(root, &metrics);

        // Update root rect height with the computed content height
        if let Some(root_rect) = self.rects.get_mut(&root) {
            root_rect.height = content_height.max(0i32);
        }

        self.perf_nodes_reflowed_last = reflowed_count as u64;
        // Ensure not const-eligible and signal that something changed
        if reflowed_count > 0 {
            self.dirty_rects.push(LayoutRect {
                x: 0,
                y: 0,
                width: metrics.container_width,
                height: content_height.max(0i32),
            });
        }
        compiler_fence(Ordering::SeqCst);
        reflowed_count
    }

    /// Layout direct block children under `root` using the provided container metrics.
    fn layout_block_children(&mut self, root: NodeKey, metrics: &ContainerMetrics) -> (usize, i32) {
        let mut reflowed_count = 0usize;
        let mut y_cursor: i32 = 0;
        if let Some(child_list) = self.children.get(&root).cloned() {
            let mut previous_bottom_margin: i32 = 0;
            for (index, child_key) in child_list.into_iter().enumerate() {
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

                let margin_left = style.margin.left as i32;
                let margin_right = style.margin.right as i32;
                let margin_top = style.margin.top as i32;
                let margin_bottom = style.margin.bottom as i32;

                let collapsed_vertical_margin =
                    if index == 0 && metrics.padding_top == 0i32 && metrics.border_top == 0i32 {
                        Self::collapse_margins_pair(metrics.margin_top, margin_top)
                    } else {
                        Self::collapse_margins_pair(previous_bottom_margin, margin_top)
                    };

                let x_position = metrics
                    .margin_left
                    .saturating_add(metrics.border_left)
                    .saturating_add(metrics.padding_left)
                    .saturating_add(margin_left.max(0i32));
                let y_position = y_cursor.saturating_add(collapsed_vertical_margin.max(0i32));

                let horizontal_margins =
                    margin_left.max(0i32).saturating_add(margin_right.max(0i32));
                let mut used_border_box_width = metrics
                    .container_width
                    .saturating_sub(horizontal_margins)
                    .max(0i32);
                used_border_box_width = Self::resolve_used_border_box_width(
                    &style,
                    metrics.container_width,
                    horizontal_margins,
                    used_border_box_width,
                );

                let (x_adjust, y_adjust) = Self::apply_relative_offsets(&style);

                let computed_height = Self::resolve_used_border_box_height(&style);
                self.rects.insert(
                    child_key,
                    LayoutRect {
                        x: x_position.saturating_add(x_adjust),
                        y: y_position.saturating_add(y_adjust),
                        width: used_border_box_width,
                        height: computed_height,
                    },
                );
                reflowed_count = reflowed_count.saturating_add(1);
                y_cursor = y_position.saturating_add(computed_height);
                previous_bottom_margin = margin_bottom;
            }
        }
        (reflowed_count, y_cursor)
    }

    #[inline]
    /// Convert a specified width plus box-sizing into a used border-box width, respecting min/max and container availability.
    fn resolve_used_border_box_width(
        style: &ComputedStyle,
        container_width: i32,
        horizontal_margins: i32,
        current_width: i32,
    ) -> i32 {
        let mut width_out = current_width;
        if let Some(specified_w) = style.width {
            let mut content_w = specified_w as i32;
            if let Some(min_w) = style.min_width {
                content_w = content_w.max(min_w as i32);
            }
            if let Some(max_w) = style.max_width {
                content_w = content_w.min(max_w as i32);
            }
            let horizontal_padding = (style.padding.left + style.padding.right).max(0.0f32) as i32;
            let horizontal_borders =
                (style.border_width.left + style.border_width.right).max(0.0f32) as i32;
            width_out = match style.box_sizing {
                BoxSizing::BorderBox => content_w,
                BoxSizing::ContentBox => content_w
                    .saturating_add(horizontal_padding)
                    .saturating_add(horizontal_borders),
            };
            width_out = width_out.min(container_width.saturating_sub(horizontal_margins).max(0i32));
        }
        width_out
    }

    #[inline]
    /// Convert a specified height plus box-sizing into a used border-box height, respecting min/max.
    fn resolve_used_border_box_height(style: &ComputedStyle) -> i32 {
        if let Some(specified_h) = style.height {
            let mut content_h = specified_h as i32;
            if let Some(min_h) = style.min_height {
                content_h = content_h.max(min_h as i32);
            }
            if let Some(max_h) = style.max_height {
                content_h = content_h.min(max_h as i32);
            }
            let vertical_padding = (style.padding.top + style.padding.bottom).max(0.0f32) as i32;
            let vertical_borders =
                (style.border_width.top + style.border_width.bottom).max(0.0f32) as i32;
            return match style.box_sizing {
                BoxSizing::BorderBox => content_h,
                BoxSizing::ContentBox => content_h
                    .saturating_add(vertical_padding)
                    .saturating_add(vertical_borders),
            };
        }
        0i32
    }

    #[inline]
    /// Compute relative x/y adjustments from `top/left/right/bottom` when `position: relative`.
    const fn apply_relative_offsets(style: &ComputedStyle) -> (i32, i32) {
        if !matches!(style.position, Position::Relative) {
            return (0i32, 0i32);
        }
        let mut x_adjust = 0i32;
        let mut y_adjust = 0i32;
        if let Some(left_off) = style.left {
            x_adjust = x_adjust.saturating_add(left_off as i32);
        }
        if let Some(right_off) = style.right {
            x_adjust = x_adjust.saturating_sub(right_off as i32);
        }
        if let Some(top_off) = style.top {
            y_adjust = y_adjust.saturating_add(top_off as i32);
        }
        if let Some(bottom_off) = style.bottom {
            y_adjust = y_adjust.saturating_sub(bottom_off as i32);
        }
        (x_adjust, y_adjust)
    }

    #[inline]
    /// Choose the layout root: first block under `#document`; if it is `html`, prefer its `body` child.
    fn choose_layout_root(&self) -> Option<NodeKey> {
        let mut root = self.find_first_block_under(NodeKey::ROOT)?;
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
        Some(root)
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

    #[inline]
    /// Returns true if there are any dirty rectangles pending since the last layout tick.
    pub const fn has_material_dirty(&self) -> bool {
        !self.dirty_rects.is_empty()
    }

    #[inline]
    /// Record a noop layout tick for callers that advance time without changes.
    pub fn mark_noop_layout_tick(&mut self) {
        // Keep counters consistent with a noop frame and provide a fence to
        // discourage accidental constant-folding in release.
        self.perf_nodes_reflowed_last = 0;
        self.perf_dirty_subtrees_last = 0;
        compiler_fence(Ordering::SeqCst);
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
                let entry = self.children.entry(parent).or_default();
                if !entry.contains(&node) {
                    entry.push(node);
                }
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
