#[inline]
fn bands_for_inputs(&self, loop_ctx: &PlaceLoopCtx<'_>, inputs: &ProcessChildIn) -> (i32, i32) {
    let parent_is_bfc = self
        .computed_styles
        .get(&loop_ctx.root)
        .is_some_and(establishes_bfc);
    if parent_is_bfc {
        (0i32, 0i32)
    } else {
        self.compute_float_bands_for_y(loop_ctx, inputs.index, inputs.y_cursor)
    }
}
#[inline]
/// Decide the resulting cursor/margin/last_info after laying out a child. Floats are out-of-flow.
fn flow_result_after_layout(
    &self,
    inputs: &ProcessChildIn,
    y_calc: i32,
    mb_calc: i32,
) -> (i32, i32, Option<(NodeKey, i32, i32)>) {
    let style = self
        .computed_styles
        .get(&inputs.child_key)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    if matches!(style.float, Float::Left | Float::Right) {
        (inputs.y_cursor, inputs.previous_bottom_margin, None)
    } else {
        (
            y_calc,
            mb_calc,
            self.last_info_for_child(inputs.child_key, mb_calc),
        )
    }
}
/// Spec reference: <https://www.w3.org/TR/CSS22>
mod box_tree; // display tree flattening
pub(crate) mod orchestrator;
mod sizing;
/// Shared types (e.g., `LayoutRect`).
mod types;
mod visual_formatting; // grouped visual formatting modules: vertical/horizontal/height/root // box sizing helpers // shared types split out from lib.rs

pub use crate::types::LayoutNodeKind;
pub use crate::types::LayoutRect;
use crate::types::{
    ChildContentCtx, ChildLayoutCtx, CollapsedPos, ContainerMetrics, HeightExtras,
    HeightsAndMargins, HeightsCtx, HorizontalEdges, PlaceLoopCtx, RootHeightsCtx, SnapshotEntry,
    TopEdges, VertCommit, VertLog,
};
use crate::visual_formatting::dimensions;
use crate::visual_formatting::vertical::establishes_bfc;
use ::core::mem::take;
use anyhow::Error;
use css::types as css_types;
use css_box::{BoxSides, compute_box_sides};
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use log::debug;
use std::collections::HashMap;
use style_engine::{Clear, ComputedStyle, Float, Position};

/// Initial containing block width used for layout when no viewport is available.
pub(crate) const INITIAL_CONTAINING_BLOCK_WIDTH: i32 = 800;
/// Fixed vertical scrollbar gutter used to approximate Chromium on Windows.
/// This is subtracted from the initial containing block width when computing the
/// root container metrics so the available inline size matches Chromium.
pub(crate) const SCROLLBAR_GUTTER_PX: i32 = 16;

/// Last placed child info used to compute the parent's content bottom per §10.6.3.
/// Tuple contents: (child key, rect bottom (y + height), effective outgoing margin-bottom).
type LastPlacedInfo = Option<(NodeKey, i32, i32)>;

/// Compact result for the block-children placement loop:
/// (`placed_count`, `y_end`, `last_positive_bottom_margin`, `last_placed_info`)
type PlaceLoopResult = (usize, i32, i32, LastPlacedInfo);

/// Internal result bundle used by `layout_block_children` to keep the function compact.
///
/// - `placed`: number of children reflowed
/// - `y_end`: final vertical cursor after placement loop
/// - `last_mb`: last positive bottom margin (clamped to >= 0) for root usage tracking
/// - `last_info`: last placed child info (key, rect bottom, effective outgoing bottom margin)
/// - `parent_edge_collapsible`: whether the parent's top edge is collapsible
struct LocalRes {
    /// Number of children reflowed
    placed: usize,
    /// Final vertical cursor after placement
    y_end: i32,
    /// Last outgoing bottom margin (signed)
    last_mb: i32,
    /// Last placed child info (key, rect bottom, outgoing mb)
    last_info: LastPlacedInfo,
    /// Whether the parent's top edge is collapsible
    parent_edge_collapsible: bool,
}

/// Compact return for `process_one_child` within the placement loop.
struct ProcessChildOut {
    /// Next y-cursor after processing the child (unchanged for floats).
    y_next: i32,
    /// Next previous-bottom-margin after processing the child (unchanged for floats).
    mb_next: i32,
    /// Last placed in-flow child's info: (key, rect bottom, outgoing bottom margin).
    last_info: Option<(NodeKey, i32, i32)>,
    /// Updated left-side clearance floor (from floats).
    left_floor_next: i32,
    /// Updated right-side clearance floor (from floats).
    right_floor_next: i32,
}

/// Compact inputs for per-child processing in the placement loop.
#[derive(Copy, Clone)]
struct ProcessChildIn {
    /// Index within the parent's block children.
    index: usize,
    /// The child node to layout.
    child_key: NodeKey,
    /// Incoming y-cursor value.
    y_cursor: i32,
    /// Previous sibling's outgoing bottom margin.
    previous_bottom_margin: i32,
    /// Masked left clearance floor (0 when parent is a BFC).
    masked_left: i32,
    /// Masked right clearance floor (0 when parent is a BFC).
    masked_right: i32,
    /// Current left clearance floor before masking (for updating from floats).
    current_left: i32,
    /// Current right clearance floor before masking (for updating from floats).
    current_right: i32,
}

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

    // Horizontal solving moved to `horizontal` module (CSS 2.2 §10.3.3).

    #[inline]
    /// Returns the tag name for a block node, or `None` if not a block.
    /// Spec: CSS 2.2 §9.4.1 — identify element boxes participating in BFC.
    fn tag_of(&self, key: NodeKey) -> Option<String> {
        let kind = self.nodes.get(&key)?.clone();
        match kind {
            LayoutNodeKind::Block { tag } => Some(tag),
            _ => None,
        }
    }

        #[inline]
        /// Find the first block-level node under `start` using a depth-first search.
        /// Spec: CSS 2.2 §9.4.1 — block formatting.
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

        // Find the last block-level node helper removed: now lives under
        // visual_formatting::vertical as a local helper where required.

        #[inline]
        /// Creates a new `Layouter` with default state.
        ///
        /// Spec: CSS 2.2 — Block formatting context entry and box tree basics
        ///   - <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>
        ///   - <https://www.w3.org/TR/CSS22/box.html>
        pub fn new() -> Self {
            let mut state = Self::default();
            // Seed with a document root so snapshots have an anchor
            state.nodes.insert(NodeKey::ROOT, LayoutNodeKind::Document);
            state
        }
        #[inline]
        /// Returns a shallow snapshot of the known nodes.
        ///
        /// Spec: Mirrors the element box tree used by block formatting contexts (simplified)
        ///   - CSS 2.2 §9.4.1 Block formatting context basics
        ///     <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>
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
        /// Non-normative: test/serializer support API (not from the CSS spec).
        pub fn attrs_map(&self) -> HashMap<NodeKey, HashMap<String, String>> {
            self.attrs.clone()
        }
        #[inline]
        /// Sets the active stylesheet.
        ///
        /// Non-normative plumbing for layout; styles originate from the Cascade/Style Engine.
        pub fn set_stylesheet(&mut self, stylesheet: css_types::Stylesheet) {
            self.stylesheet = stylesheet;
        }

        #[inline]
        /// Replaces the current computed-style map.
        ///
        /// Non-normative plumbing for layout; computed styles are inputs per CSS Cascade.
        ///   - CSS 2.2 Cascade (reference): <https://www.w3.org/TR/CSS22/cascade.html>
        pub fn set_computed_styles(&mut self, map: HashMap<NodeKey, ComputedStyle>) {
            self.computed_styles = map;
        }

        #[inline]
        /// Computes a naive block layout and returns the number of nodes affected.
        ///
        /// Spec: CSS 2.2 — Block formatting and vertical margin collapsing (subset)
        ///   - Block layout loop: <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>
        ///   - Collapsing margins: <https://www.w3.org/TR/CSS22/box.html#collapsing-margins>
        pub fn compute_layout(&mut self) -> usize {
            orchestrator::compute_layout_impl(self)
        }

        /// Layout direct block children under `root` using the provided container metrics.
        /// Returns `(reflowed_count, total_content_height, last_outgoing_bottom_margin, last_placed_info)` where
        /// `last_placed_info` is `Some((last_key, rect_bottom, margin_bottom_out))` for the last placed in-flow block.
        fn layout_block_children(
            &mut self,
            root: NodeKey,
            metrics: &ContainerMetrics,
            ancestor_applied_at_edge: bool,
        ) -> (usize, i32, i32, LastPlacedInfo) {
            let block_children = self.collect_block_children(root);
            let mut first_collapsed_top_positive: i32 = 0;
            let res: LocalRes = if block_children.is_empty() {
                LocalRes {
                    placed: 0,
                    y_end: 0,
                    last_mb: 0,
                    last_info: None,
                    parent_edge_collapsible: true,
                }
            } else {
                let (loop_ctx, parent_edge_collapsible) = self.prepare_place_loop(
                    root,
                    metrics,
                    &block_children,
                    ancestor_applied_at_edge,
                );
                let (placed, y_end, last_mb, last_info): PlaceLoopResult =
                    self.place_block_children_loop(loop_ctx, &mut first_collapsed_top_positive);
                LocalRes {
                    placed,
                    y_end,
                    last_mb,
                    last_info,
                    parent_edge_collapsible,
                }
            };

            // CSS 2.2 §8.3.1 & §9.4.1:
            // - Subtract the positive collapsed-top absorbed at the parent's top edge when that edge
            //   is collapsible (no padding/border and no BFC).
            // Determine if the parent's bottom edge is collapsible (no bottom padding/border and no BFC).
            let root_style = self
                .computed_styles
                .get(&root)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let bottom_edge_collapsible = root_style.padding.bottom.max(0.0) as i32 == 0i32
                && root_style.border_width.bottom.max(0.0) as i32 == 0i32
                && !establishes_bfc(&root_style);
            // Include the last outgoing bottom margin only when the parent's bottom edge is not collapsible.
            let y_end_to_bottom_margin_edge = if bottom_edge_collapsible {
                res.y_end
            } else {
                res.y_end.saturating_add(res.last_mb)
            };
            let adjusted_content_height = if res.parent_edge_collapsible {
                y_end_to_bottom_margin_edge
                    .saturating_sub(first_collapsed_top_positive)
                    .max(0i32)
            } else {
                y_end_to_bottom_margin_edge.max(0i32)
            };
            (
                res.placed,
                adjusted_content_height,
                res.last_mb,
                res.last_info,
            )
        }

        /// Build the ordered list of block-level children under `root`, honoring display flattening.
        #[inline]
        pub(crate) fn collect_block_children(&self, root: NodeKey) -> Vec<NodeKey> {
            let child_list =
                box_tree::flatten_display_children(&self.children, &self.computed_styles, root);
            let mut block_children: Vec<NodeKey> = Vec::new();
            for key in child_list {
                if matches!(self.nodes.get(&key), Some(&LayoutNodeKind::Block { .. })) {
                    block_children.push(key);
                }
            }
            block_children
        }

        #[inline]
        /// Prepare the placement loop context, applying leading top collapse and updating parent rect y when applicable.
        fn prepare_place_loop<'children>(
            &mut self,
            root: NodeKey,
            metrics: &ContainerMetrics,
            block_children: &'children [NodeKey],
            ancestor_applied_at_edge: bool,
        ) -> (PlaceLoopCtx<'children>, bool) {
            let (y_start, prev_bottom_after, leading_applied, skipped) =
                visual_formatting::vertical::apply_leading_top_collapse(
                    self,
                    root,
                    metrics,
                    block_children,
                    ancestor_applied_at_edge,
                );
            debug!(
                "[VERT-GROUP apply root={root:?}] y_start={y_start} prev_bottom_after={prev_bottom_after} leading_applied={leading_applied} skip_count={skipped}"
            );
            let (parent_sides, parent_edge_collapsible) =
                self.build_parent_edge_context(root, metrics);
            if leading_applied != 0i32
                && parent_edge_collapsible
                && let Some(parent_rect) = self.rects.get_mut(&root)
            {
                parent_rect.y = y_start;
            }
            let loop_ctx = PlaceLoopCtx {
                root,
                metrics: *metrics,
                block_children,
                y_cursor: y_start,
                prev_bottom_after,
                leading_applied,
                skipped,
                parent_sides,
                parent_edge_collapsible,
                ancestor_applied_at_edge,
            };
            (loop_ctx, parent_edge_collapsible)
        }

        /// Place block children, skipping leading structurally-empty boxes per `skipped`.
        #[inline]
        fn place_block_children_loop(
            &mut self,
            loop_ctx: PlaceLoopCtx<'_>,
            first_collapsed_top_positive: &mut i32,
        ) -> PlaceLoopResult {
            let mut reflowed_count = 0usize;
            let mut previous_bottom_margin: i32 = 0;
            let mut y_cursor = loop_ctx.y_cursor;
            // Track side-specific clearance floors from preceding floats.
            let mut clearance_floor_left_y: i32 = 0;
            let mut clearance_floor_right_y: i32 = 0;
            let mut last_placed_info: Option<(NodeKey, i32, i32)> = None;
            for (index, child_key) in loop_ctx.block_children.iter().copied().enumerate() {
                // Deterministically suppress placement and margin application for leading structurally-empty boxes.
                if index < loop_ctx.skipped {
                    self.commit_zero_height_leading(index, child_key, &loop_ctx, y_cursor);
                    reflowed_count = reflowed_count.saturating_add(1);
                    continue;
                }
                // If the parent establishes a BFC, external float floors do not apply to any child.
                let parent_is_bfc = self
                    .computed_styles
                    .get(&loop_ctx.root)
                    .is_some_and(establishes_bfc);
                let (masked_left, masked_right) = if parent_is_bfc {
                    (0i32, 0i32)
                } else {
                    (clearance_floor_left_y, clearance_floor_right_y)
                };
                let proc = self.process_one_child(
                    &loop_ctx,
                    &ProcessChildIn {
                        index,
                        child_key,
                        y_cursor,
                        previous_bottom_margin,
                        masked_left,
                        masked_right,
                        current_left: clearance_floor_left_y,
                        current_right: clearance_floor_right_y,
                    },
                    first_collapsed_top_positive,
                );
                reflowed_count = reflowed_count.saturating_add(1);
                y_cursor = proc.y_next;
                previous_bottom_margin = proc.mb_next;
                last_placed_info = proc.last_info;
                clearance_floor_left_y = proc.left_floor_next;
                clearance_floor_right_y = proc.right_floor_next;
            }
            (
                reflowed_count,
                y_cursor,
                previous_bottom_margin,
                last_placed_info,
            )
        }

        #[inline]
        /// Per-child processing for `place_block_children_loop` to keep the loop small and readable.
        fn process_one_child(
            &mut self,
            loop_ctx: &PlaceLoopCtx<'_>,
            inputs: &ProcessChildIn,
            first_collapsed_top_positive: &mut i32,
        ) -> ProcessChildOut {
            // Determine horizontal float-avoidance bands for this y position.
            let (band_left, band_right) = self.bands_for_inputs(loop_ctx, inputs);
            let clearance_floor_y = self.compute_clearance_floor_for_child(
                inputs.child_key,
                inputs.masked_left,
                inputs.masked_right,
            );
            let ctx =
                self.build_child_ctx(loop_ctx, inputs, clearance_floor_y, band_left, band_right);
            let (y_calc, mb_calc) = self.layout_child_and_advance(
                loop_ctx.root,
                inputs.child_key,
                ctx,
                first_collapsed_top_positive,
            );
            let (y_next, mb_next, last_info) =
                self.flow_result_after_layout(inputs, y_calc, mb_calc);
            // Update clearance floors by side if this child floats, using the unmasked running floors.
            let (left_floor_next, right_floor_next) = self.update_clearance_floors_for_float(
                inputs.child_key,
                inputs.current_left,
                inputs.current_right,
            );
            ProcessChildOut {
                y_next,
                mb_next,
                last_info,
                left_floor_next,
                right_floor_next,
            }
        }

        /// Commit a zero-height rectangle for a leading structurally-empty child to preserve width without affecting flow.
        #[inline]
        fn commit_zero_height_leading(
            &mut self,
            index: usize,
            child_key: NodeKey,
            loop_ctx: &PlaceLoopCtx<'_>,
            y_cursor: i32,
        ) {
            let style = self
                .computed_styles
                .get(&child_key)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let sides = compute_box_sides(&style);
            let (used_bb_w, child_x, child_y, x_adjust, y_adjust) = Self::prepare_child_position(
                &style,
                &sides,
                &ChildLayoutCtx {
                    index,
                    is_first_placed: false,
                    metrics: loop_ctx.metrics,
                    y_cursor,
                    previous_bottom_margin: 0,
                    parent_self_top_margin: 0,
                    leading_top_applied: 0,
                    ancestor_applied_at_edge_for_children: true,
                    parent_edge_collapsible: loop_ctx.parent_edge_collapsible,
                    // Leading empties are suppressed; no clearance applied.
                    clearance_floor_y: 0i32,
                    float_band_left: 0,
                    float_band_right: 0,
                },
                0,
            );
            self.commit_vert(VertCommit {
                index,
                prev_mb: 0,
                margin_top_raw: sides.margin_top,
                margin_top_eff: 0,
                eff_bottom: 0,
                is_empty: true,
                collapsed_top: 0,
                parent_origin_y: Self::parent_content_origin(&loop_ctx.metrics).1,
                y_position: child_y,
                y_cursor_in: y_cursor,
                leading_top_applied: 0,
                child_key,
                rect: LayoutRect {
                    x: child_x.saturating_add(x_adjust),
                    y: child_y.saturating_add(y_adjust),
                    width: used_bb_w,
                    height: 0,
                },
            });
        }

        /// Lay out a single block-level child and return `(height, y_position, margin_bottom)`.
        fn layout_one_block_child(
            &mut self,
            child_key: NodeKey,
            ctx: ChildLayoutCtx,
        ) -> (i32, i32, i32) {
            let has_style = self.computed_styles.contains_key(&child_key);
            debug!("[LAYOUT][DIAG] child={child_key:?} has_computed_style={has_style}");
            let style = self
                .computed_styles
                .get(&child_key)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let sides = compute_box_sides(&style);
            let CollapsedPos {
                margin_top_eff,
                collapsed_top,
                used_bb_w,
                child_x,
                child_y,
                x_adjust,
                y_adjust,
            } = self.compute_collapsed_and_position(child_key, &ctx, &style, &sides);
            let HeightsAndMargins {
                computed_h,
                eff_bottom,
                is_empty,
                margin_bottom_out,
            } = self.compute_heights_and_margins(HeightsCtx {
                child_key,
                style: &style,
                sides,
                child_x,
                child_y,
                used_bb_w,
                ctx: &ctx,
                margin_top_eff,
            });
            debug!(
                "[VERT child place idx={}] first={} ancestor_applied_at_edge_for_children={} mt_raw={} mt_eff={} collapsed_top={} is_empty={} parent_origin_y={} y_cursor_in={} -> y={} mb_out={} lt_applied={}",
                ctx.index,
                ctx.is_first_placed,
                ctx.ancestor_applied_at_edge_for_children,
                sides.margin_top,
                margin_top_eff,
                collapsed_top,
                is_empty,
                Self::parent_content_origin(&ctx.metrics).1,
                ctx.y_cursor,
                child_y,
                margin_bottom_out,
                ctx.leading_top_applied
            );
            self.commit_vert(VertCommit {
                index: ctx.index,
                prev_mb: ctx.previous_bottom_margin,
                margin_top_raw: sides.margin_top,
                margin_top_eff,
                eff_bottom,
                is_empty,
                collapsed_top,
                parent_origin_y: Self::parent_content_origin(&ctx.metrics).1,
                y_position: child_y,
                y_cursor_in: ctx.y_cursor,
                leading_top_applied: if ctx.index == 0 {
                    ctx.leading_top_applied
                } else {
                    0
                },
                child_key,
                rect: LayoutRect {
                    x: child_x.saturating_add(x_adjust),
                    y: child_y.saturating_add(y_adjust),
                    width: used_bb_w,
                    height: computed_h,
                },
            });
            (computed_h, child_y, margin_bottom_out)
        }

        #[inline]
        /// Compute collapsed top offset and initial position info for a child.
        fn compute_collapsed_and_position(
            &self,
            child_key: NodeKey,
            ctx: &ChildLayoutCtx,
            style: &ComputedStyle,
            sides: &BoxSides,
        ) -> CollapsedPos {
            let margin_top_eff =
                visual_formatting::vertical::effective_child_top_margin(self, child_key, sides);
            let collapsed_top = Self::compute_collapsed_vertical_margin(ctx, margin_top_eff, style);
            let (used_bb_w, child_x, child_y, x_adjust, y_adjust) =
                Self::prepare_child_position(style, sides, ctx, collapsed_top);
            CollapsedPos {
                margin_top_eff,
                collapsed_top,
                used_bb_w,
                child_x,
                child_y,
                x_adjust,
                y_adjust,
            }
        }

        #[inline]
        /// Compute heights and outgoing margin values for a child.
        fn compute_heights_and_margins(&mut self, hctx: HeightsCtx<'_>) -> HeightsAndMargins {
            let (content_h_inner, _last_out_mb) =
                self.compute_child_content_height(ChildContentCtx {
                    key: hctx.child_key,
                    used_border_box_width: hctx.used_bb_w,
                    sides: hctx.sides,
                    x: hctx.child_x,
                    y: hctx.child_y,
                    ancestor_applied_at_edge: hctx.ctx.ancestor_applied_at_edge_for_children,
                });
            // Child's own used height is computed from its content box; do not include
            // its outgoing bottom margin here. The parent accounts for the bottom margin edge.
            let content_h = content_h_inner;
            let computed_h = visual_formatting::height::compute_used_height(
                self,
                hctx.style,
                hctx.child_key,
                HeightExtras {
                    padding_top: hctx.sides.padding_top,
                    padding_bottom: hctx.sides.padding_bottom,
                    border_top: hctx.sides.border_top,
                    border_bottom: hctx.sides.border_bottom,
                },
                content_h,
            );
            let eff_bottom = visual_formatting::vertical::effective_child_bottom_margin(
                self,
                hctx.child_key,
                &hctx.sides,
            );
            let is_empty =
                self.is_effectively_empty_box(hctx.style, &hctx.sides, computed_h, hctx.child_key);
            let margin_bottom_out = if is_empty && hctx.ctx.is_first_placed {
                Self::compute_first_placed_empty_margin_bottom(
                    hctx.ctx.previous_bottom_margin,
                    hctx.ctx.parent_self_top_margin,
                    hctx.margin_top_eff,
                    eff_bottom,
                )
            } else {
                Self::compute_margin_bottom_out(hctx.margin_top_eff, eff_bottom, is_empty)
            };
            HeightsAndMargins {
                computed_h,
                eff_bottom,
                is_empty,
                margin_bottom_out,
            }
        }

        #[inline]
        /// Prepare child's used width and initial position based on horizontal solving and relative offsets.
        /// Spec: CSS 2.2 §10.3.3 (width) and §9.4.3 (relative positioning adjustments).
        fn prepare_child_position(
            style: &ComputedStyle,
            sides: &BoxSides,
            ctx: &ChildLayoutCtx,
            collapsed_top: i32,
        ) -> (i32, i32, i32, i32, i32) {
            let (parent_x, parent_y) = Self::parent_content_origin(&ctx.metrics);
            // Apply horizontal float-avoidance bands to available width.
            let available_width = ctx
                .metrics
                .container_width
                .saturating_sub(ctx.float_band_left)
                .saturating_sub(ctx.float_band_right)
                .max(0i32);
            let (used_bb_w_raw, resolved_ml, _resolved_mr) =
                visual_formatting::horizontal::solve_block_horizontal(
                    style,
                    sides,
                    available_width,
                    sides.margin_left,
                    sides.margin_right,
                );
            let (x_adjust, y_adjust) = Self::apply_relative_offsets(style);
            // Shift by left float band to move the child past the occupied area.
            let child_x = parent_x
                .saturating_add(ctx.float_band_left)
                .saturating_add(resolved_ml);
            let mut child_y = Self::compute_y_position(parent_y, ctx.y_cursor, collapsed_top);
            // Apply clearance: if a clearance floor is in effect and the element has clear set,
            // raise the child to the floor.
            if matches!(style.clear, Clear::Left | Clear::Right | Clear::Both)
                && ctx.clearance_floor_y > child_y
            {
                child_y = ctx.clearance_floor_y;
            }
            // Child border-box width cannot exceed the available width after bands.
            let used_bb_w = used_bb_w_raw.min(available_width);
            (used_bb_w, child_x, child_y, x_adjust, y_adjust)
        }

        #[inline]
        /// Compute used height for a block child (wrapper for heights module).
        fn compute_used_height(
            &self,
            style: &ComputedStyle,
            child_key: NodeKey,
            extras: HeightExtras,
            child_content_height: i32,
        ) -> i32 {
            dimensions::compute_used_height_impl(
                self,
                style,
                child_key,
                extras,
                child_content_height,
            )
        }

        #[inline]
        /// Build child metrics and compute raw content height by laying out descendants.
        /// Returns `(content_height, last_positive_bottom_margin)`.
        fn compute_child_content_height(&mut self, cctx: ChildContentCtx) -> (i32, i32) {
            dimensions::compute_child_content_height_impl(self, cctx)
        }
        /// Emit a vertical log and insert the child's rect.
        fn commit_vert(&mut self, vert_commit: VertCommit) {
            Self::log_vert(VertLog {
                index: vert_commit.index,
                prev_mb: vert_commit.prev_mb,
                margin_top_raw: vert_commit.margin_top_raw,
                margin_top_eff: vert_commit.margin_top_eff,
                eff_bottom: vert_commit.eff_bottom,
                is_empty: vert_commit.is_empty,
                collapsed_top: vert_commit.collapsed_top,
                parent_origin_y: vert_commit.parent_origin_y,
                y_position: vert_commit.y_position,
                y_cursor_in: vert_commit.y_cursor_in,
                leading_top_applied: vert_commit.leading_top_applied,
            });
            let key = vert_commit.child_key;
            let rect = vert_commit.rect;
            let x = rect.x;
            let y = rect.y;
            let width = rect.width;
            let height = rect.height;
            if let Some(attrs) = self.attrs.get(&key) {
                if let Some(id_val) = attrs.get("id") {
                    debug!(
                        "[LAYOUT][DIAG] insert_rect key={key:?} id=#{id_val} rect=({x}, {y}, {width}, {height})"
                    );
                }
            } else {
                debug!("[LAYOUT][DIAG] insert_rect key={key:?} rect=({x}, {y}, {width}, {height})");
            }
            Self::insert_child_rect(&mut self.rects, key, rect);
        }

        #[inline]
        /// Log a vertical layout step with margin collapsing inputs and results.
        fn log_vert(entry: VertLog) {
            debug!(
                "[VERT child idx={}] pm_prev_bottom={} child(mt_raw={}, mt_eff={}, mb(eff={}), empty={}) collapsed_top={} parent_origin_y={} -> y={} cursor_in={} lt_applied={}",
                entry.index,
                entry.prev_mb,
                entry.margin_top_raw,
                entry.margin_top_eff,
                entry.eff_bottom,
                entry.is_empty,
                entry.collapsed_top,
                entry.parent_origin_y,
                entry.y_position,
                entry.y_cursor_in,
                entry.leading_top_applied,
            );
        }

        #[inline]
        /// Compute the clearance floor (y) for a child based on its `clear` property and current
        /// side-specific float floors. BFC boundaries nullify external float influence.
        /// Spec: CSS 2.2 §9.5 Floats; §9.4.1 BFC and interaction with floats.
        fn compute_clearance_floor_for_child(
            &self,
            child_key: NodeKey,
            floor_left: i32,
            floor_right: i32,
        ) -> i32 {
            let style = self
                .computed_styles
                .get(&child_key)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            if establishes_bfc(&style) {
                return 0i32;
            }
            match style.clear {
                Clear::Left => floor_left,
                Clear::Right => floor_right,
                Clear::Both => floor_left.max(floor_right),
                Clear::None => 0i32,
            }
        }

        #[inline]
        /// Update side-specific float clearance floors after laying out a potential float.
        /// Returns the new `(left_floor, right_floor)` pair.
        /// Spec: CSS 2.2 §9.5 Floats.
        fn update_clearance_floors_for_float(
            &self,
            child_key: NodeKey,
            current_left: i32,
            current_right: i32,
        ) -> (i32, i32) {
            let mut left = current_left;
            let mut right = current_right;
            let Some(style) = self.computed_styles.get(&child_key) else {
                return (left, right);
            };
            if matches!(style.float, Float::None) {
                return (left, right);
            }
            let Some(rect) = self.rects.get(&child_key) else {
                return (left, right);
            };
            let mb_pos = compute_box_sides(style).margin_bottom.max(0i32);
            let bottom_edge = rect.y.saturating_add(rect.height).saturating_add(mb_pos);
            match style.float {
                Float::Left => {
                    if bottom_edge > left {
                        left = bottom_edge;
                    }
                }
                Float::Right => {
                    if bottom_edge > right {
                        right = bottom_edge;
                    }
                }
                Float::None => {}
            }
            (left, right)
        }

        #[inline]
        /// Compute horizontal float-avoidance bands at a given y for prior floats among siblings.
        /// A float contributes when its vertical span overlaps the query y.
        /// Returns `(left_band, right_band)` in pixels within the parent's content box.
        fn compute_float_bands_for_y(
            &self,
            loop_ctx: &PlaceLoopCtx<'_>,
            up_to_index: usize,
            y_in_parent: i32,
        ) -> (i32, i32) {
            let mut left_band: i32 = 0;
            let mut right_band: i32 = 0;
            let (parent_x, _parent_y) = Self::parent_content_origin(&loop_ctx.metrics);
            let parent_content_right = parent_x.saturating_add(loop_ctx.metrics.container_width);
            for (idx, key) in loop_ctx.block_children.iter().copied().enumerate() {
                if idx >= up_to_index {
                    break;
                }
                let Some(style) = self.computed_styles.get(&key) else {
                    continue;
                };
                if matches!(style.float, Float::None) {
                    continue;
                }
                let Some(rect) = self.rects.get(&key) else {
                    continue;
                };
                let top = rect.y;
                let bottom = rect.y.saturating_add(rect.height);
                if !(top <= y_in_parent && y_in_parent < bottom) {
                    continue;
                }
                match style.float {
                    Float::Left => {
                        // Band is the occupied inline-start extent within parent content.
                        let occupied = rect.x.saturating_add(rect.width).saturating_sub(parent_x);
                        if occupied > left_band {
                            left_band = occupied;
                        }
                    }
                    Float::Right => {
                        // Band is the occupied inline-end extent within parent content.
                        let occupied = parent_content_right.saturating_sub(rect.x);
                        if occupied > right_band {
                            right_band = occupied;
                        }
                    }
                    Float::None => {}
                }
            }
            (left_band.max(0i32), right_band.max(0i32))
        }

        #[inline]
        fn bands_for_inputs(
            &self,
            loop_ctx: &PlaceLoopCtx<'_>,
            inputs: &ProcessChildIn,
        ) -> (i32, i32) {
            let parent_is_bfc = self
                .computed_styles
                .get(&loop_ctx.root)
                .is_some_and(establishes_bfc);
            if parent_is_bfc {
                (0i32, 0i32)
            } else {
                self.compute_float_bands_for_y(loop_ctx, inputs.index, inputs.y_cursor)
            }
        }

        #[inline]
        /// Decide the resulting cursor/margin/last_info after laying out a child. Floats are out-of-flow.
        fn flow_result_after_layout(
            &self,
            inputs: &ProcessChildIn,
            y_calc: i32,
            mb_calc: i32,
        ) -> (i32, i32, Option<(NodeKey, i32, i32)>) {
            let style = self
                .computed_styles
                .get(&inputs.child_key)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            if matches!(style.float, Float::Left | Float::Right) {
                (inputs.y_cursor, inputs.previous_bottom_margin, None)
            } else {
                (
                    y_calc,
                    mb_calc,
                    self.last_info_for_child(inputs.child_key, mb_calc),
                )
            }
        }
    }

    #[inline]
    /// Build child metrics and compute raw content height by laying out descendants.
    /// Returns `(content_height, last_positive_bottom_margin)`.
    fn compute_child_content_height(&mut self, cctx: ChildContentCtx) -> (i32, i32) {
        dimensions::compute_child_content_height_impl(self, cctx)
    }

    #[inline]
    /// Build child context for a child.
    fn build_child_ctx(
        &self,
        loop_ctx: &PlaceLoopCtx<'_>,
        inputs: &ProcessChildIn,
        clearance_floor_y: i32,
        band_left: i32,
        band_right: i32,
    ) -> ChildLayoutCtx {
        ChildLayoutCtx {
            index: inputs.index,
            is_first_placed: inputs.index == loop_ctx.skipped,
            metrics: loop_ctx.metrics,
            y_cursor: inputs.y_cursor,
            previous_bottom_margin: if inputs.index == loop_ctx.skipped {
                loop_ctx.prev_bottom_after
            } else {
                inputs.previous_bottom_margin
            },
            parent_self_top_margin: if loop_ctx.parent_edge_collapsible
                && inputs.index == loop_ctx.skipped
                && !loop_ctx.ancestor_applied_at_edge
            {
                loop_ctx.parent_sides.margin_top
            } else {
                0
            },
            leading_top_applied: if inputs.index == loop_ctx.skipped {
                loop_ctx.leading_applied
            } else {
                0i32
            },
            ancestor_applied_at_edge_for_children: loop_ctx.ancestor_applied_at_edge
                || (loop_ctx.leading_applied != 0i32),
            parent_edge_collapsible: loop_ctx.parent_edge_collapsible,
            clearance_floor_y,
            float_band_left: band_left,
            float_band_right: band_right,
        }
    }

    #[inline]
    /// Build last placed in-flow info for diagnostics and parent bottom calculations, if the rect exists.
    fn last_info_for_child(&self, child_key: NodeKey, mb_out: i32) -> Option<(NodeKey, i32, i32)> {
        let rect = self.rects.get(&child_key)?;
        let rect_bottom = rect.y.saturating_add(rect.height);
        log::debug!("[PLACE-LOOP] child={child_key:?} rect_bottom={rect_bottom} mb_out={mb_out}");
        Some((child_key, rect_bottom, mb_out))
    }

    #[inline]
    /// Compute final outgoing bottom margin for a child, allowing internal top/bottom collapse when the child is empty.
    fn compute_margin_bottom_out(margin_top: i32, effective_bottom: i32, is_empty: bool) -> i32 {
        if is_empty {
            Self::collapse_margins_pair(margin_top, effective_bottom)
        } else {
            effective_bottom
        }
    }

    #[inline]
    /// Compute outgoing bottom margin for an empty first-placed child, collapsing any internal
    /// leading group propagated via `previous_bottom` with the parent's own top margin (if applied
    /// at the edge), the child's effective top, and the child's effective bottom.
    fn compute_first_placed_empty_margin_bottom(
        previous_bottom: i32,
        parent_self_top: i32,
        child_top_eff: i32,
        child_bottom_eff: i32,
    ) -> i32 {
        let list = [
            previous_bottom,
            parent_self_top,
            child_top_eff,
            child_bottom_eff,
        ];
        Self::collapse_margins_list(&list)
    }

    #[inline]
    /// Collapse a list of vertical margins per CSS 2.2 §8.3.1.
    /// - If all are positive, result is the largest positive.
    /// - If all are negative, result is the most negative.
    /// - Otherwise, result is (largest positive) + (most negative) (algebraic sum of extremes).
    fn collapse_margins_list(margins: &[i32]) -> i32 {
        if margins.is_empty() {
            return 0i32;
        }
        let mut max_pos = i32::MIN;
        let mut min_neg = i32::MAX;
        let mut any_pos = false;
        let mut any_neg = false;
        for &margin in margins {
            if margin >= 0i32 {
                any_pos = true;
                if margin > max_pos {
                    max_pos = margin;
                }
            } else {
                any_neg = true;
                if margin < min_neg {
                    min_neg = margin;
                }
            }
        }
        match (any_pos, any_neg) {
            (true, false) => max_pos,
            (false, true) => min_neg,
            (true, true) => max_pos.saturating_add(min_neg),
            (false, false) => 0i32,
        }
    }

    #[inline]
    /// Determine if a box is effectively empty for margin-collapsing purposes (approximation of §8.3.1).
    fn is_effectively_empty_box(
        &self,
        style: &ComputedStyle,
        sides: &BoxSides,
        used_height: i32,
        key: NodeKey,
    ) -> bool {
        // Boxes that establish a new BFC are not treated as empty for margin-collapsing
        // propagation. This prevents internal top/bottom collapse from leaking across BFCs.
        if establishes_bfc(style) {
            return false;
        }
        sides.padding_top == 0i32
            && sides.padding_bottom == 0i32
            && sides.border_top == 0i32
            && sides.border_bottom == 0i32
            && used_height == 0i32
            && !self.has_inline_text_descendant(key)
            && style.min_height.unwrap_or(0.0) as i32 == 0i32
    }

    #[inline]
    /// Compute the y position for a child box by adding the vertical cursor and any collapsed top margin.
    const fn compute_y_position(origin_y: i32, cursor: i32, collapsed_vertical_margin: i32) -> i32 {
        // Spec: allow negative collapsed top margins to pull the box upward.
        // CSS 2.2 §8.3.1 — do not clamp the collapsed vertical margin to zero.
        origin_y
            .saturating_add(cursor)
            .saturating_add(collapsed_vertical_margin)
    }

    #[inline]
    /// Debug: log first placed child's context at layout time.
    fn log_first_child_context(root: NodeKey, ctx: &ChildLayoutCtx) {
        debug!(
            "[VERT-CONTEXT first root={root:?}] pad_top={} border_top={} parent_self_top={} prev_bottom={} y_cursor={} lt_applied={}",
            ctx.metrics.padding_top,
            ctx.metrics.border_top,
            ctx.parent_self_top_margin,
            ctx.previous_bottom_margin,
            ctx.y_cursor,
            ctx.leading_top_applied
        );
    }

    #[inline]
    /// Compute parent's box sides and whether its top edge is collapsible (no padding/border).
    fn build_parent_edge_context(
        &self,
        root: NodeKey,
        _metrics: &ContainerMetrics,
    ) -> (BoxSides, bool) {
        let parent_style = self
            .computed_styles
            .get(&root)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let parent_sides = compute_box_sides(&parent_style);
        // CSS 2.2 §8.3.1 Collapsing margins — a parent's top margin may collapse with its
        // first block child's top margin only if the parent's top edge is collapsible and the
        // parent does not establish a new block formatting context.
        // References:
        //   - https://www.w3.org/TR/CSS22/box.html#collapsing-margins
        //   - https://www.w3.org/TR/CSS22/visuren.html#block-formatting
        // We must use the parent's actual box sides (padding/border) rather than container
        // metrics, and ensure that BFC-establishing parents do not allow collapse.
        let parent_edge_collapsible = parent_sides.padding_top == 0i32
            && parent_sides.border_top == 0i32
            && !visual_formatting::vertical::establishes_bfc(&parent_style);
        (parent_sides, parent_edge_collapsible)
    }

    #[inline]
    /// Update the running maximum of the first positive collapsed top margin absorbed at a parent's top edge.
    fn record_first_collapsed_top_positive(
        parent_edge_collapsible: bool,
        index: usize,
        y_position: i32,
        parent_content_origin_y: i32,
        acc: &mut i32,
    ) {
        if parent_edge_collapsible && index == 0 {
            let added = y_position.saturating_sub(parent_content_origin_y);
            *acc = (*acc).max(added.max(0i32));
        }
    }

    #[inline]
    /// Layout one child and advance the y-cursor and previous bottom margin, updating diagnostics.
    fn layout_child_and_advance(
        &mut self,
        root: NodeKey,
        child_key: NodeKey,
        ctx: ChildLayoutCtx,
        first_collapsed_top_positive: &mut i32,
    ) -> (i32, i32) {
        if ctx.is_first_placed {
            Self::log_first_child_context(root, &ctx);
        }
        let (computed_height, y_position, margin_bottom) =
            self.layout_one_block_child(child_key, ctx);
        let parent_content_origin_y = ctx
            .metrics
            .margin_top
            .saturating_add(ctx.metrics.border_top)
            .saturating_add(ctx.metrics.padding_top);
        let y_cursor_next = y_position
            .saturating_sub(parent_content_origin_y)
            .saturating_add(computed_height);
        // Only record a positive collapsed-top absorbed at the parent edge if the parent edge
        // is collapsible per CSS 2.2 §8.3.1 and the parent does not establish a BFC (§9.4.1).
        Self::record_first_collapsed_top_positive(
            ctx.parent_edge_collapsible,
            ctx.index,
            y_position,
            parent_content_origin_y,
            first_collapsed_top_positive,
        );
        (y_cursor_next, margin_bottom)
    }

    #[inline]
    /// Compute the vertical offset from collapsed margins above a block child.
    fn compute_collapsed_vertical_margin(
        ctx: &ChildLayoutCtx,
        child_margin_top: i32,
        _child_style: &ComputedStyle,
    ) -> i32 {
        if ctx.is_first_placed {
            if ctx.ancestor_applied_at_edge_for_children {
                // An ancestor already applied at an outer edge. Do not re-apply at this edge.
                // Collapsing with the internal leading group is handled for propagation via
                // previous_bottom_margin in margin_bottom_out; placement offset here is zero.
                debug!("[VERT-COLLAPSE first skip] ancestor_applied_at_edge -> collapsed_top=0");
                return 0i32;
            }
            if ctx.leading_top_applied != 0i32 {
                // The leading-top collapse was already applied at the parent edge.
                debug!(
                    "[VERT-COLLAPSE first] lt_applied={} -> collapsed_top=0",
                    ctx.leading_top_applied
                );
                return 0i32;
            }
            // CSS 2.2 §8.3.1 & §9.4.1: Only allow parent/first-child collapse at the parent top
            // edge when the parent's top edge is collapsible (no padding/border) AND the parent
            // does not establish a BFC.
            if ctx.parent_edge_collapsible {
                // Collapse parent's own top margin with child's top margin and apply only the
                // incremental amount beyond the parent's own top margin at the parent's top edge.
                let combined =
                    Self::collapse_margins_pair(ctx.parent_self_top_margin, child_margin_top);
                let inc = combined.saturating_sub(ctx.parent_self_top_margin);
                debug!(
                    "[VERT-COLLAPSE first edge] parent_top={} child_top={} -> combined={} inc={}",
                    ctx.parent_self_top_margin, child_margin_top, combined, inc
                );
                return inc;
            }
        }
        let pair = Self::collapse_margins_pair(ctx.previous_bottom_margin, child_margin_top);
        debug!(
            "[VERT-COLLAPSE sibling] prev_mb={} child_top={} -> collapsed_top={}",
            ctx.previous_bottom_margin, child_margin_top, pair
        );
        pair
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

    // compute_fill_available_width removed; horizontal solving is handled by solve_block_horizontal

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
            total_border_box_width: used_border_box_width,
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
    /// Spec: CSS 2.2 §9.4.1 — simplified content detection for empty/inline checks.
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
        take(&mut self.dirty_rects)
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
    pub const fn hit_test(&self, _x: i32, _y: i32) -> Option<NodeKey> {
        None
    }
    #[inline]
    /// Marks the given nodes as having dirty style.
    pub const fn mark_nodes_style_dirty(&self, _nodes: &[NodeKey]) {
        /* no-op shim */
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
    pub const fn mark_noop_layout_tick(&mut self) {
        self.perf_nodes_reflowed_last = 0;
        self.perf_dirty_subtrees_last = 0;
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
