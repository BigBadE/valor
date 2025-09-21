//! External layouter shim used by tests to compute simple block layout.
//! Spec reference: <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>
// helper moved into impl Layouter below
/// Spec: <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>
mod box_tree;
mod sizing;

use crate::sizing::{used_border_box_height, used_border_box_width};
use anyhow::Error;
use core::mem::take;
use core::sync::atomic::{Ordering, compiler_fence};
use css::types as css_types;
use css_box::compute_box_sides;
use css_text::default_line_height_px;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;
use style_engine::{ComputedStyle, Position};

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

/// Context for computing content and border-box heights for the root element.
#[derive(Clone, Copy)]
struct RootHeightsCtx {
    /// The root node key being laid out.
    root: NodeKey,
    /// Container metrics of the root's content box.
    metrics: ContainerMetrics,
    /// Final y position for the root after top-margin collapse handling.
    root_y: i32,
    /// Last positive bottom margin reported by child layout to include when needed.
    root_last_pos_mb: i32,
    /// Maximum bottom extent of children (including positive bottom margins), if any.
    content_bottom: Option<i32>,
}

/// Horizontal padding and border widths for a child box (in pixels, clamped >= 0).
#[derive(Clone, Copy)]
struct HorizontalEdges {
    /// Child padding-left in pixels.
    padding_left: i32,
    /// Child padding-right in pixels.
    padding_right: i32,
    /// Child border-left in pixels.
    border_left: i32,
    /// Child border-right in pixels.
    border_right: i32,
}

/// Top padding and border widths for a child box (in pixels, clamped >= 0).
#[derive(Clone, Copy)]
struct TopEdges {
    /// Child padding-top in pixels.
    padding_top: i32,
    /// Child border-top in pixels.
    border_top: i32,
}

/// Context for laying out a single block child.
#[derive(Clone, Copy)]
struct ChildLayoutCtx {
    /// Index of the child in block flow order.
    index: usize,
    /// Container metrics of the parent content box.
    metrics: ContainerMetrics,
    /// Current vertical cursor (y offset) within the parent content box.
    y_cursor: i32,
    /// Bottom margin of the previous block sibling (for margin collapsing).
    previous_bottom_margin: i32,
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
/// Chrome headless with an 800px window reports body client width ~784px.
const INITIAL_CONTAINING_BLOCK_WIDTH: i32 = 784;

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
    /// Children per parent in DOM order (elements and text tracked for inline basics).
    children: HashMap<NodeKey, Vec<NodeKey>>,
    /// Text contents for text nodes (`InsertText`).
    text_by_node: HashMap<NodeKey, String>,
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
    /// Resolve margin-left/right when specified as 'auto' in the shim.
    ///
    /// Rules (simplified shim consistent with §10.3.3 and our available inputs):
    /// - Both auto: resolve both to 0.
    /// - One auto: resolve it so that `margin_left` + `border_box_width` + `margin_right` == `container_content_width`,
    ///   clamped to >= 0. The non-auto side keeps its specified (possibly negative) margin.
    fn resolve_horizontal_auto_margins(
        style: &ComputedStyle,
        container_content_width: i32,
        used_border_box_width: i32,
        margin_left: i32,
        margin_right: i32,
    ) -> (i32, i32) {
        let left_auto = style.margin_left_auto;
        let right_auto = style.margin_right_auto;
        if left_auto && right_auto {
            return (0i32, 0i32);
        }
        if left_auto ^ right_auto {
            let other = if left_auto { margin_right } else { margin_left };
            let remaining = container_content_width
                .saturating_sub(used_border_box_width)
                .saturating_sub(other);
            let resolved = remaining.max(0i32);
            if left_auto {
                return (resolved, margin_right);
            }
            return (margin_left, resolved);
        }
        (margin_left, margin_right)
    }
    #[inline]
    /// Find the first block-level node under `start` using a depth-first search.
    ///
    /// Spec: CSS 2.2 §9.4.1 — identify element boxes that participate in block formatting contexts.
    fn find_first_block_under(&self, start: NodeKey) -> Option<NodeKey> {
        if matches!(self.nodes.get(&start), Some(&LayoutNodeKind::Block { .. })) {
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

        let sides = compute_box_sides(&root_style);
        let padding_left = sides.padding_left;
        let padding_right = sides.padding_right;
        let padding_top = sides.padding_top;
        let border_left = sides.border_left;
        let border_right = sides.border_right;
        let border_top = sides.border_top;
        let margin_left = sides.margin_left;
        let margin_top = sides.margin_top;

        // CSS 2.2 §8.1 Box model: margins lie outside the border.
        // CSS 2.2 §10.1 Containing block: the content/padding edge forms the containing block,
        // not including margins. Therefore, do NOT subtract margins from the available width.
        let horizontal_non_content = padding_left
            .saturating_add(padding_right)
            .saturating_add(border_left)
            .saturating_add(border_right);
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

        // Emit a preliminary rect for the root itself (border-box width; height is 0 for now)
        // Y offset is adjusted below after we consider parent–first-child top margin collapse.
        self.rects.insert(
            root,
            LayoutRect {
                x: 0,
                y: 0,
                width: metrics.container_width,
                height: 0,
            },
        );

        let (reflowed_count, _content_height_from_cursor, root_last_pos_mb) =
            self.layout_block_children(root, &metrics);

        // Determine root y after potential top-margin collapse with the first block child.
        let root_y = Self::compute_root_y_after_top_collapse(self, root, &metrics);

        // Compute content extents from child rects and derive content height.
        let (content_top, content_bottom) = self.aggregate_content_extents(root);
        let root_y_aligned = if metrics.padding_top == 0i32
            && metrics.border_top == 0i32
            && let Some(top_value) = content_top
        {
            top_value.max(0i32)
        } else {
            root_y
        };
        let (content_height, root_height_border_box) = self.compute_root_heights(RootHeightsCtx {
            root,
            metrics,
            root_y: root_y_aligned,
            root_last_pos_mb,
            content_bottom,
        });

        self.update_root_rect(root, &metrics, root_y_aligned, root_height_border_box);

        self.perf_nodes_reflowed_last = reflowed_count as u64;
        // Ensure not const-eligible and signal that something changed
        self.push_dirty_rect_if_changed(metrics.container_width, content_height, reflowed_count);
        compiler_fence(Ordering::SeqCst);
        reflowed_count
    }

    /// Compute the root y position after collapsing the parent's top margin with the first child's top margin when eligible.
    fn compute_root_y_after_top_collapse(&self, root: NodeKey, metrics: &ContainerMetrics) -> i32 {
        if metrics.padding_top == 0i32
            && metrics.border_top == 0i32
            && let Some(child_list) = self.children.get(&root)
            && let Some(&first_child) = child_list
                .iter()
                .find(|&&key| matches!(self.nodes.get(&key), Some(&LayoutNodeKind::Block { .. })))
        {
            let first_style = self
                .computed_styles
                .get(&first_child)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let collapsed =
                Self::collapse_margins_pair(metrics.margin_top, first_style.margin.top as i32);
            return collapsed.max(0i32);
        }
        metrics.margin_top
    }

    /// Aggregate the minimum top and maximum bottom (including positive bottom margin) across block children.
    fn aggregate_content_extents(&self, root: NodeKey) -> (Option<i32>, Option<i32>) {
        let mut content_top: Option<i32> = None;
        let mut content_bottom: Option<i32> = None;
        if let Some(children) = self.children.get(&root) {
            for child_key in children {
                if matches!(
                    self.nodes.get(child_key),
                    Some(LayoutNodeKind::Block { .. })
                ) && let Some(rect) = self.rects.get(child_key)
                {
                    content_top =
                        Some(content_top.map_or(rect.y, |current_top| current_top.min(rect.y)));
                    let bottom_margin = self
                        .computed_styles
                        .get(child_key)
                        .map_or(0i32, |style| style.margin.bottom as i32)
                        .max(0i32);
                    let bottom = rect
                        .y
                        .saturating_add(rect.height)
                        .saturating_add(bottom_margin);
                    content_bottom = Some(
                        content_bottom.map_or(bottom, |current_bottom| current_bottom.max(bottom)),
                    );
                }
            }
        }
        (content_top, content_bottom)
    }

    /// Compute content height and root border-box height.
    fn compute_root_heights(&self, ctx: RootHeightsCtx) -> (i32, i32) {
        let content_origin = ctx
            .root_y
            .saturating_add(ctx.metrics.border_top)
            .saturating_add(ctx.metrics.padding_top);
        let content_bottom_with_parent_mb = ctx
            .content_bottom
            .map(|bottom_value| bottom_value.saturating_add(ctx.root_last_pos_mb));
        let content_height = content_bottom_with_parent_mb.map_or(0i32, |bottom_value| {
            bottom_value.saturating_sub(content_origin).max(0i32)
        });

        let root_style = self
            .computed_styles
            .get(&ctx.root)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let padding_bottom = root_style.padding.bottom.max(0.0f32) as i32;
        let border_bottom = root_style.border_width.bottom.max(0.0f32) as i32;
        let root_height_border_box = content_height
            .saturating_add(ctx.metrics.padding_top)
            .saturating_add(padding_bottom)
            .saturating_add(ctx.metrics.border_top)
            .saturating_add(border_bottom)
            .max(0i32);
        (content_height, root_height_border_box)
    }

    /// Update the root rectangle with final y and height.
    fn update_root_rect(
        &mut self,
        root: NodeKey,
        metrics: &ContainerMetrics,
        root_y: i32,
        root_height_border_box: i32,
    ) {
        if let Some(root_rect) = self.rects.get_mut(&root) {
            root_rect.x = metrics.margin_left;
            root_rect.y = root_y;
            root_rect.height = root_height_border_box;
        }
    }

    /// Push a dirty rectangle when reflow changed any nodes.
    fn push_dirty_rect_if_changed(
        &mut self,
        width: i32,
        content_height: i32,
        reflowed_count: usize,
    ) {
        if reflowed_count > 0 {
            self.dirty_rects.push(LayoutRect {
                x: 0,
                y: 0,
                width,
                height: content_height.max(0i32),
            });
        }
    }

    /// Layout direct block children under `root` using the provided container metrics.
    /// Returns `(reflowed_count, total_content_height, last_positive_bottom_margin)`.
    fn layout_block_children(
        &mut self,
        root: NodeKey,
        metrics: &ContainerMetrics,
    ) -> (usize, i32, i32) {
        let mut reflowed_count = 0usize;
        let mut y_cursor: i32 = 0;
        let mut first_collapsed_top_positive: i32 = 0;
        let mut last_positive_bottom_margin: i32 = 0;
        // Select children while honoring display generation: skip `display:none`,
        // and treat `display:contents` as passthrough by lifting its children.
        let child_list =
            box_tree::flatten_display_children(&self.children, &self.computed_styles, root);
        if !child_list.is_empty() {
            let mut previous_bottom_margin: i32 = 0;
            // Consider only element (block) children for block layout ordering.
            let mut block_children: Vec<NodeKey> = Vec::new();
            for key in child_list {
                if matches!(self.nodes.get(&key), Some(LayoutNodeKind::Block { .. })) {
                    block_children.push(key);
                }
            }
            for (index, child_key) in block_children.into_iter().enumerate() {
                let ctx = ChildLayoutCtx {
                    index,
                    metrics: *metrics,
                    y_cursor,
                    previous_bottom_margin,
                };
                let (computed_height, y_position, margin_bottom) =
                    self.layout_one_block_child(child_key, ctx);
                reflowed_count = reflowed_count.saturating_add(1);
                let parent_content_origin_y = metrics
                    .margin_top
                    .saturating_add(metrics.border_top)
                    .saturating_add(metrics.padding_top);
                y_cursor = y_position
                    .saturating_sub(parent_content_origin_y)
                    .saturating_add(computed_height);
                // Record the positive collapsed top margin absorbed at the top edge (index 0)
                if index == 0 && metrics.padding_top == 0i32 && metrics.border_top == 0i32 {
                    // The amount added to y_position beyond parent_content_origin_y is the collapsed positive.
                    let added = y_position.saturating_sub(parent_content_origin_y);
                    first_collapsed_top_positive =
                        first_collapsed_top_positive.max(added.max(0i32));
                }
                previous_bottom_margin = margin_bottom;
            }
            last_positive_bottom_margin = previous_bottom_margin.max(0i32);
        }
        // Exclude the positive collapsed top margin from the parent's content height (§8.3.1)
        let adjusted_content_height = y_cursor
            .saturating_sub(first_collapsed_top_positive)
            .max(0i32);
        (
            reflowed_count,
            adjusted_content_height,
            last_positive_bottom_margin,
        )
    }

    #[inline]
    /// Lay out a single block-level child and return `(height, y_position, margin_bottom)`.
    /// Helper methods below keep this function concise for clippy.
    fn layout_one_block_child(
        &mut self,
        child_key: NodeKey,
        ctx: ChildLayoutCtx,
    ) -> (i32, i32, i32) {
        let style = self
            .computed_styles
            .get(&child_key)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);

        let sides = compute_box_sides(&style);
        let margin_left = sides.margin_left;
        let margin_right = sides.margin_right;
        let margin_top = sides.margin_top;
        let margin_bottom = sides.margin_bottom;

        let collapsed_vertical_margin = if ctx.index == 0
            && ctx.metrics.padding_top == 0i32
            && ctx.metrics.border_top == 0i32
        {
            // Parent's margin_top is not available here (metrics.margin_top carries absolute baseline),
            // so collapse child's top margin with zero per container-level handling.
            Self::collapse_margins_pair(0, margin_top)
        } else {
            Self::collapse_margins_pair(ctx.previous_bottom_margin, margin_top)
        };

        let (parent_content_origin_x, parent_content_origin_y) =
            Self::parent_content_origin(&ctx.metrics);

        // Resolve auto margins per CSS 2.2 §10.3.3 (shim rules):
        // - If both left/right are auto, resolve both to 0 and let width consume available.
        // - If exactly one is auto, resolve it so the sum fits the containing block width; clamp to >= 0.
        let fill_available_width =
            Self::compute_fill_available_width(&ctx.metrics, margin_left, margin_right);
        let used_border_box_width = used_border_box_width(&style, fill_available_width);
        let (resolved_margin_left, _resolved_margin_right) = Self::resolve_horizontal_auto_margins(
            &style,
            ctx.metrics.container_width,
            used_border_box_width,
            margin_left,
            margin_right,
        );

        let x_position = parent_content_origin_x.saturating_add(resolved_margin_left);
        let y_position = Self::compute_y_position(
            parent_content_origin_y,
            ctx.y_cursor,
            collapsed_vertical_margin,
        );

        // used_border_box_width computed above

        let (x_adjust, y_adjust) = Self::apply_relative_offsets(&style);

        // Recurse into block children with the child's content metrics (relative offsets do not affect descendants).
        let child_metrics = Self::build_child_metrics(
            used_border_box_width,
            HorizontalEdges {
                padding_left: sides.padding_left,
                padding_right: sides.padding_right,
                border_left: sides.border_left,
                border_right: sides.border_right,
            },
            TopEdges {
                padding_top: sides.padding_top,
                border_top: sides.border_top,
            },
            x_position,
            y_position,
        );
        let (_, mut child_content_height, child_last_pos_mb) =
            self.layout_block_children(child_key, &child_metrics);
        // If this container has bottom padding or border, the last child's positive bottom margin
        // is inside the content area (no bottom collapsing with parent). Include it in content height.
        if (sides.padding_bottom > 0i32 || sides.border_bottom > 0i32) && child_last_pos_mb > 0i32 {
            child_content_height = child_content_height.saturating_add(child_last_pos_mb);
        }

        // Compute used height; if unspecified (auto), derive from children or minimal text support.
        let mut computed_height = used_border_box_height(&style);
        if style.height.is_none() {
            computed_height = child_content_height
                .saturating_add(sides.padding_top)
                .saturating_add(sides.padding_bottom)
                .saturating_add(sides.border_top)
                .saturating_add(sides.border_bottom);
            if computed_height == 0i32 && self.has_inline_text_descendant(child_key) {
                computed_height = default_line_height_px(&style);
            }
        }

        // Insert or update the child's border-box rect now that height is known.
        let rect_y = y_position.saturating_add(y_adjust);
        Self::insert_child_rect(
            &mut self.rects,
            child_key,
            LayoutRect {
                x: x_position.saturating_add(x_adjust),
                y: rect_y,
                width: used_border_box_width,
                height: computed_height,
            },
        );
        (computed_height, y_position, margin_bottom)
    }

    #[inline]
    fn compute_y_position(origin_y: i32, cursor: i32, collapsed_vertical_margin: i32) -> i32 {
        origin_y
            .saturating_add(cursor)
            .saturating_add(collapsed_vertical_margin.max(0i32))
    }

    /// Compute the parent's content origin from its margins, borders, and padding.
    const fn parent_content_origin(metrics: &ContainerMetrics) -> (i32, i32) {
        let x = metrics
            .margin_left
            .saturating_add(metrics.border_left)
            .saturating_add(metrics.padding_left);
        let y = metrics
            .margin_top
            .saturating_add(metrics.border_top)
            .saturating_add(metrics.padding_top);
        (x, y)
    }

    /// Compute the fill-available width for a child by subtracting positive horizontal margins.
    fn compute_fill_available_width(
        metrics: &ContainerMetrics,
        margin_left: i32,
        margin_right: i32,
    ) -> i32 {
        let horizontal_margins = margin_left.max(0i32).saturating_add(margin_right.max(0i32));
        metrics
            .container_width
            .saturating_sub(horizontal_margins)
            .max(0i32)
    }

    /// Build `ContainerMetrics` for a child from its used width and edge aggregates.
    fn build_child_metrics(
        used_border_box_width: i32,
        horizontal: HorizontalEdges,
        top: TopEdges,
        x_position: i32,
        y_position: i32,
    ) -> ContainerMetrics {
        ContainerMetrics {
            container_width: used_border_box_width
                .saturating_sub(horizontal.padding_left)
                .saturating_sub(horizontal.padding_right)
                .saturating_sub(horizontal.border_left)
                .saturating_sub(horizontal.border_right)
                .max(0i32),
            padding_left: horizontal.padding_left,
            padding_top: top.padding_top,
            border_left: horizontal.border_left,
            border_top: top.border_top,
            margin_left: x_position,
            margin_top: y_position,
        }
    }

    #[inline]
    /// Insert or update the child's rectangle in the layout map.
    fn insert_child_rect(
        rects: &mut HashMap<NodeKey, LayoutRect>,
        child_key: NodeKey,
        rect: LayoutRect,
    ) {
        rects.insert(child_key, rect);
    }

    #[inline]
    /// Returns true if the node has any inline text descendant recorded via `InsertText`.
    fn has_inline_text_descendant(&self, key: NodeKey) -> bool {
        let mut stack: Vec<NodeKey> = match self.children.get(&key) {
            Some(kids) => kids.clone(),
            None => return false,
        };
        while let Some(current) = stack.pop() {
            let node_kind = self.nodes.get(&current).cloned();
            if matches!(node_kind, Some(LayoutNodeKind::InlineText { .. })) {
                return true;
            }
            if matches!(
                node_kind,
                Some(LayoutNodeKind::Block { .. } | LayoutNodeKind::Document)
            ) && let Some(children) = self.children.get(&current)
            {
                stack.extend(children.iter().copied());
            }
        }
        false
    }

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
            .is_some_and(|tag_name| tag_name.eq_ignore_ascii_case("html"));
        if root_is_html
            && let Some(child_list) = self.children.get(&root)
            && let Some(body_child) = child_list.iter().copied().find(|candidate| {
                self.tag_of(*candidate)
                    .is_some_and(|tag_name| tag_name.eq_ignore_ascii_case("body"))
            })
        {
            root = body_child;
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
            DOMUpdate::InsertText {
                parent, node, text, ..
            } => {
                // Track inline text nodes for minimal text layout.
                self.nodes
                    .insert(node, LayoutNodeKind::InlineText { text: text.clone() });
                self.text_by_node.insert(node, text);
                let entry = self.children.entry(parent).or_default();
                if !entry.contains(&node) {
                    entry.push(node);
                }
            }
            DOMUpdate::EndOfDocument => { /* ignore */ }
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
        }
        Ok(())
    }
}
