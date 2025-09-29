//! Layouter: block formatting, margin collapsing, and minimal float placement.
//!
//! Primary references: CSS 2.2 §8.3.1 (Collapsing margins), §9.4.1 (Block formatting context),
//! §9.5 (Floats), and §10.3.3 (Width of non-replaced elements).
//! This module coordinates block children layout, float avoidance bands, and clearance floors.
/// Spec reference: <https://www.w3.org/TR/CSS22>
/// Spec-driven chapter/section modules live under dedicated directories; deprecated `chapters` module removed.
pub(crate) mod orchestrator;
// Chapter modules mapped to CSS 2.2 spec structure. These map numeric folders to valid identifiers.
#[path = "10_visual_details/mod.rs"]
mod chapter10;
#[path = "8_box_model/mod.rs"]
mod chapter8;
#[path = "9_visual_formatting/mod.rs"]
mod chapter9;

// Core types are defined in this file; external uses can import from css_core::types via re-exports.
use crate::orchestrator::place_child::PlacedBlock;
use anyhow::Error;
use chapter8::part_8_3_1_collapsing_margins as cm83;
use chapter9::part_9_4_1_block_formatting_context::establishes_block_formatting_context;
use chapter10::part_10_1_containing_block as cb10;
use core::mem::take;
use css_box::{BoxSides, compute_box_sides};
use css_display::normalize_children;
use css_orchestrator::style_model::{Clear, ComputedStyle, Float};
use css_orchestrator::types as css_types;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use log::debug;
use std::collections::HashMap;

// =====================
// Core data types (moved from types.rs)
// =====================

/// Initial viewport height used for root percent-height resolution (approximation).
/// This mirrors the initial containing block width usage and is kept in sync with renderer defaults.
pub const INITIAL_CONTAINING_BLOCK_HEIGHT: i32 = 768;

use css_orchestrator::style_model::ComputedStyle as _CoreComputedStyleForTypes;

/// A rectangle in device-independent pixels (border-box space). Uses f32 for subpixel precision.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    /// X coordinate of the border-box origin.
    pub x: f32,
    /// Y coordinate of the border-box origin.
    pub y: f32,
    /// Border-box width.
    pub width: f32,
    /// Border-box height.
    pub height: f32,
}

/// Metrics for the container box edges and available content width.
#[derive(Clone, Copy, Debug)]
pub struct ContainerMetrics {
    /// Content box width available to children inside the container.
    pub container_width: i32,
    /// Total border-box width of the container (e.g., viewport width adjusted for scrollbars).
    pub total_border_box_width: i32,
    /// Container padding-left in pixels (clamped to >= 0).
    pub padding_left: i32,
    /// Container padding-top in pixels (clamped to >= 0).
    pub padding_top: i32,
    /// Container border-left width in pixels (clamped to >= 0).
    pub border_left: i32,
    /// Container border-top width in pixels (clamped to >= 0).
    pub border_top: i32,
    /// Container margin-left in pixels (may be negative).
    pub margin_left: i32,
    /// Container margin-top in pixels (may be negative).
    pub margin_top: i32,
}

/// Context for placing block children under a root.
#[derive(Clone, Copy)]
pub struct PlaceLoopCtx<'pl> {
    /// The root node whose children are being placed.
    pub root: NodeKey,
    /// Container metrics for the root's content box.
    pub metrics: ContainerMetrics,
    /// The ordered list of block children to place.
    pub block_children: &'pl [NodeKey],
    /// Incoming y cursor for placement.
    pub y_cursor: i32,
    /// Previous bottom margin after leading-group propagation for the first placed child.
    pub prev_bottom_after: i32,
    /// Leading-top value applied at the parent edge (if any).
    pub leading_applied: i32,
    /// Number of leading structurally-empty children to suppress.
    pub skipped: usize,
    /// Parent sides for first-child incremental calculations.
    pub parent_sides: BoxSides,
    /// Whether the parent's top edge is collapsible.
    pub parent_edge_collapsible: bool,
    /// Whether an ancestor already applied the leading collapse at an outer edge.
    pub ancestor_applied_at_edge: bool,
    /// Index of the first in-flow (non-float) block child to place; used for parent/child top collapse.
    pub first_inflow_index: usize,
}

/// Aggregated results for collapsed margins and initial child position.
#[derive(Clone, Copy)]
pub struct CollapsedPos {
    /// Effective top margin after internal propagation through empties.
    pub margin_top_eff: i32,
    /// Collapsed top offset applied at this edge.
    pub collapsed_top: i32,
    /// Used border-box width for the child.
    pub used_bb_w: i32,
    /// Child x-position (margin edge).
    pub child_x: i32,
    /// Child y-position (margin edge).
    pub child_y: i32,
    /// Relative x adjustment from position:relative.
    pub x_adjust: i32,
    /// Relative y adjustment from position:relative.
    pub y_adjust: i32,
    /// True if clearance lifted the child beyond the collapsed-top pre-position.
    pub clear_lifted: bool,
}

/// Inputs for computing a child's heights and outgoing bottom margin.
#[derive(Clone, Copy)]
pub struct HeightsCtx<'heights> {
    /// The child node key whose heights are being computed.
    pub child_key: NodeKey,
    /// The computed style of the child.
    pub style: &'heights _CoreComputedStyleForTypes,
    /// Box sides (padding/border/margins) snapshot for the child.
    pub sides: BoxSides,
    /// Child x position (margin edge).
    pub child_x: i32,
    /// Child y position (margin edge).
    pub child_y: i32,
    /// Used border-box width for the child.
    pub used_bb_w: i32,
    /// Parent context for the child layout.
    pub ctx: &'heights ChildLayoutCtx,
    /// Effective top margin used in outgoing bottom margin calculation.
    pub margin_top_eff: i32,
}

/// Aggregated results for a child's computed height and margins.
#[derive(Clone, Copy)]
pub struct HeightsAndMargins {
    /// Final computed border-box height for the child.
    pub computed_h: i32,
    /// Effective bottom margin after internal propagation.
    pub eff_bottom: i32,
    /// Whether the child is considered empty for collapsing.
    pub is_empty: bool,
    /// Outgoing bottom margin from the child to the next sibling.
    pub margin_bottom_out: i32,
}

/// Bundle for committing vertical results and rectangle for a child.
#[derive(Clone, Copy)]
pub struct VertCommit {
    /// Child index within parent's block children.
    pub index: usize,
    /// Previous sibling bottom margin (pre-collapsed).
    pub prev_mb: i32,
    /// Raw top margin from computed sides.
    pub margin_top_raw: i32,
    /// Effective top margin after collapsing through empties.
    pub margin_top_eff: i32,
    /// Effective bottom margin after collapsing through empties.
    pub eff_bottom: i32,
    /// Whether the child is effectively empty for collapsing.
    pub is_empty: bool,
    /// Collapsed top offset applied at this edge.
    pub collapsed_top: i32,
    /// Parent content origin y.
    pub parent_origin_y: i32,
    /// Final y position for the child.
    pub y_position: i32,
    /// Incoming y cursor in parent content space.
    pub y_cursor_in: i32,
    /// Leading-top collapse applied at parent edge for the first child, if any.
    pub leading_top_applied: i32,
    /// The child node key.
    pub child_key: NodeKey,
    /// Final border-box rectangle for the child.
    pub rect: LayoutRect,
}

/// Context for computing a child's content height by laying out its descendants.
#[derive(Clone, Copy)]
pub struct ChildContentCtx {
    /// The child node key whose descendants will be laid out.
    pub key: NodeKey,
    /// Child used border-box width.
    pub used_border_box_width: i32,
    /// Box sides (padding/border/margins) snapshot for the child.
    pub sides: BoxSides,
    /// Child x position (margin edge).
    pub x: i32,
    /// Child y position (margin edge).
    pub y: i32,
    /// Whether an ancestor has already applied the leading top collapse at its edge.
    pub ancestor_applied_at_edge: bool,
}

/// Inputs captured for vertical layout logs.
#[derive(Clone, Copy)]
pub struct VertLog {
    /// Index of the child within the parent block children.
    pub index: usize,
    /// Previous sibling's bottom margin (pre-collapsed).
    pub prev_mb: i32,
    /// Child raw top margin from computed sides.
    pub margin_top_raw: i32,
    /// Child effective top margin used for collapse with parent/previous.
    pub margin_top_eff: i32,
    /// Child effective bottom margin (post internal propagation through empties).
    pub eff_bottom: i32,
    /// Whether the child is considered empty for vertical collapsing.
    pub is_empty: bool,
    /// Result of top margin collapsing applied at this edge.
    pub collapsed_top: i32,
    /// Parent content origin y.
    pub parent_origin_y: i32,
    /// Final chosen y position for the child.
    pub y_position: i32,
    /// Incoming y cursor in the parent content space.
    pub y_cursor_in: i32,
    /// Leading-top collapse applied at parent edge (if any) for first child.
    pub leading_top_applied: i32,
}

/// Context for computing content and border-box heights for the root element.
#[derive(Clone, Copy)]
pub struct RootHeightsCtx {
    /// The root node key being laid out.
    pub root: NodeKey,
    /// Container metrics of the root's content box.
    pub metrics: ContainerMetrics,
    /// Final y position for the root after top-margin collapse handling.
    pub root_y: i32,
    /// Last positive bottom margin reported by child layout to include when needed.
    pub root_last_pos_mb: i32,
    /// Maximum bottom extent of children (including positive bottom margins), if any.
    pub content_bottom: Option<i32>,
}

/// Horizontal padding and border widths for a child box (in pixels, clamped >= 0).
#[derive(Clone, Copy)]
pub struct HorizontalEdges {
    /// Child padding-left in pixels.
    pub padding_left: i32,
    /// Child padding-right in pixels.
    pub padding_right: i32,
    /// Child border-left in pixels.
    pub border_left: i32,
    /// Child border-right in pixels.
    pub border_right: i32,
}

/// Top padding and border widths for a child box (in pixels, clamped >= 0).
#[derive(Clone, Copy)]
pub struct TopEdges {
    /// Child padding-top in pixels.
    pub padding_top: i32,
    /// Child border-top in pixels.
    pub border_top: i32,
}

/// Vertical padding and border widths for height calculations.
#[derive(Clone, Copy)]
pub struct HeightExtras {
    /// Child padding-top in pixels.
    pub padding_top: i32,
    /// Child padding-bottom in pixels.
    pub padding_bottom: i32,
    /// Child border-top in pixels.
    pub border_top: i32,
    /// Child border-bottom in pixels.
    pub border_bottom: i32,
}

/// Context for laying out a single block child.
#[derive(Clone, Copy)]
pub struct ChildLayoutCtx {
    /// Index of the child in block flow order.
    pub index: usize,
    /// True if this is the first placed child (after skipping leading empties).
    pub is_first_placed: bool,
    /// Container metrics of the parent content box.
    pub metrics: ContainerMetrics,
    /// Current vertical cursor (y offset) within the parent content box.
    pub y_cursor: i32,
    /// Bottom margin of the previous block sibling (for margin collapsing).
    pub previous_bottom_margin: i32,
    /// Parent's own top margin to include when collapsing with the first child's top.
    pub parent_self_top_margin: i32,
    /// Leading top collapse applied at parent edge (from a leading empty chain), if any.
    pub leading_top_applied: i32,
    /// Whether an ancestor (this parent) already applied the leading collapse at its edge.
    /// This must be propagated to all child subtrees, including leading empty ones,
    /// so they do not re-apply at their own top edge.
    pub ancestor_applied_at_edge_for_children: bool,
    /// Whether the parent top edge is collapsible for parent/first-child margin collapse per CSS 2.2
    /// §8.3.1, taking into account padding/border and BFC creation per §9.4.1.
    pub parent_edge_collapsible: bool,
    /// Clearance floor in parent content space: minimum y that content must start at due to
    /// preceding floats on the relevant sides. Simplified: we track a single floor for any clear.
    pub clearance_floor_y: i32,
    /// Horizontal avoidance band width on the left from active left floats at this y.
    pub float_band_left: i32,
    /// Horizontal avoidance band width on the right from active right floats at this y.
    pub float_band_right: i32,
}

/// Kinds of layout nodes known to the layouter.
#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    /// The root document node.
    Document,
    /// A block-level element.
    Block { tag: String },
    /// An inline text node.
    InlineText { text: String },
}

/// A convenience type alias for snapshot entries returned by [`Layouter::snapshot`].
pub type SnapshotEntry = (NodeKey, LayoutNodeKind, Vec<NodeKey>);

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
/// (`placed_count`, `y_end`, `last_positive_bottom_margin`, `last_placed_info`, `leading_collapse_contrib`)
type PlaceLoopResult = (usize, i32, i32, LastPlacedInfo, i32);

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
    /// Leading-collapse contribution reported by the first in-flow child (0 otherwise).
    leading_collapse_contrib: i32,
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
    /// Leading-collapse contribution from the first in-flow child (0 otherwise).
    leading_collapse_contrib: i32,
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
    // Horizontal solving moved to `horizontal` module (CSS 2.2 §10.3.3).

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
    /// Compute the y used to query float-avoidance bands for a child and return
    /// `(y_for_bands, collapsed_top, margin_top_eff)`.
    fn band_query_y_for_child(
        &self,
        loop_ctx: &PlaceLoopCtx<'_>,
        inputs: &ProcessChildIn,
        style: &ComputedStyle,
        clearance_floor_y: i32,
    ) -> (i32, i32, i32) {
        if matches!(style.clear, Clear::Left | Clear::Right | Clear::Both) {
            return (clearance_floor_y, 0, 0);
        }
        let sides = compute_box_sides(style);
        let margin_top_eff =
            cm83::effective_child_top_margin_public(self, inputs.child_key, &sides);
        let is_first = inputs.index == loop_ctx.first_inflow_index;
        let tmp_ctx = ChildLayoutCtx {
            index: inputs.index,
            is_first_placed: is_first,
            metrics: loop_ctx.metrics,
            y_cursor: inputs.y_cursor,
            previous_bottom_margin: if is_first {
                if loop_ctx.parent_edge_collapsible {
                    loop_ctx.prev_bottom_after
                } else {
                    0
                }
            } else {
                inputs.previous_bottom_margin
            },
            parent_self_top_margin: if loop_ctx.parent_edge_collapsible
                && is_first
                && !loop_ctx.ancestor_applied_at_edge
            {
                loop_ctx.parent_sides.margin_top
            } else {
                0
            },
            leading_top_applied: if loop_ctx.parent_edge_collapsible && is_first {
                loop_ctx.leading_applied
            } else {
                0
            },
            ancestor_applied_at_edge_for_children: loop_ctx.ancestor_applied_at_edge,
            parent_edge_collapsible: loop_ctx.parent_edge_collapsible,
            clearance_floor_y: 0,
            float_band_left: 0,
            float_band_right: 0,
        };
        let collapsed_top =
            cm83::compute_collapsed_vertical_margin_public(&tmp_ctx, margin_top_eff, style);
        (
            inputs.y_cursor.saturating_add(collapsed_top),
            collapsed_top,
            margin_top_eff,
        )
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
        let res: LocalRes = if block_children.is_empty() {
            LocalRes {
                placed: 0,
                y_end: 0,
                last_mb: 0,
                last_info: None,
                leading_collapse_contrib: 0,
            }
        } else {
            let (loop_ctx, _parent_edge_collapsible) =
                self.prepare_place_loop(root, metrics, &block_children, ancestor_applied_at_edge);
            let (placed, y_end, last_mb, last_info, leading_collapse_contrib): PlaceLoopResult =
                self.place_block_children_loop(loop_ctx);
            LocalRes {
                placed,
                y_end,
                last_mb,
                last_info,
                leading_collapse_contrib,
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
            && !establishes_block_formatting_context(&root_style);
        // Include the last outgoing bottom margin only when the parent's bottom edge is not collapsible.
        let y_end_to_bottom_margin_edge = if bottom_edge_collapsible {
            res.y_end
        } else {
            res.y_end.saturating_add(res.last_mb)
        };
        // Subtract leading-collapse contribution when applicable. The contribution is guaranteed
        // to be zero when the parent edge is not collapsible.
        let adjusted_content_height = y_end_to_bottom_margin_edge
            .saturating_sub(res.leading_collapse_contrib)
            .max(0i32);
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
        let child_list = normalize_children(&self.children, &self.computed_styles, root);
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
        let (y_cursor_start, prev_bottom_after, leading_applied, skipped) =
            chapter8::part_8_3_1_collapsing_margins::apply_leading_top_collapse_public(
                self,
                root,
                metrics,
                block_children,
                ancestor_applied_at_edge,
            );
        debug!(
            "[VERT-GROUP apply root={root:?}] y_cursor_start={y_cursor_start} prev_bottom_after={prev_bottom_after} leading_applied={leading_applied} skip_count={skipped}"
        );
        let (parent_sides, parent_edge_collapsible) = self.build_parent_edge_context(root, metrics);
        // When applying the leading collapse at the parent's top edge, reflect this shift both
        // in the parent's rect (for viewport-relative geometry) and in the placement loop's
        // metrics so that children's parent_content_origin includes the applied offset.
        let mut metrics_for_children = *metrics;
        if leading_applied != 0i32 && parent_edge_collapsible {
            if let Some(parent_rect) = self.rects.get_mut(&root) {
                parent_rect.y = leading_applied as f32;
            }
            metrics_for_children.margin_top = leading_applied;
        }
        // Determine the first in-flow child index (ignoring floats) starting at `skipped`.
        let mut first_inflow_index = usize::MAX;
        for (idx, key) in block_children.iter().copied().enumerate() {
            if idx < skipped {
                continue;
            }
            let style = self
                .computed_styles
                .get(&key)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            if matches!(style.float, Float::None) {
                first_inflow_index = idx;
                break;
            }
        }
        let loop_ctx = PlaceLoopCtx {
            root,
            metrics: metrics_for_children,
            block_children,
            y_cursor: y_cursor_start,
            prev_bottom_after,
            leading_applied,
            skipped,
            parent_sides,
            parent_edge_collapsible,
            ancestor_applied_at_edge,
            first_inflow_index,
        };
        debug!(
            "[PARENT-EDGE ctx root={root:?}] parent_edge_collapsible={parent_edge_collapsible} first_inflow_index={first_inflow_index} y_cursor_start={y_cursor_start} prev_bottom_after={prev_bottom_after} leading_applied={leading_applied} skipped={skipped}"
        );
        (loop_ctx, parent_edge_collapsible)
    }

    /// Place block children, skipping leading structurally-empty boxes per `skipped`.
    #[inline]
    fn place_block_children_loop(&mut self, loop_ctx: PlaceLoopCtx<'_>) -> PlaceLoopResult {
        let mut reflowed_count = 0usize;
        let mut previous_bottom_margin: i32 = 0;
        let mut y_cursor = loop_ctx.y_cursor;
        // Track side-specific clearance floors from preceding floats.
        let mut clearance_floor_left_y: i32 = 0;
        let mut clearance_floor_right_y: i32 = 0;
        let mut last_placed_info: Option<(NodeKey, i32, i32)> = None;
        let mut leading_collapse_contrib: i32 = 0;
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
                .is_some_and(establishes_block_formatting_context);
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
            );
            reflowed_count = reflowed_count.saturating_add(1);
            y_cursor = proc.y_next;
            previous_bottom_margin = proc.mb_next;
            last_placed_info = proc.last_info;
            clearance_floor_left_y = proc.left_floor_next;
            clearance_floor_right_y = proc.right_floor_next;
            if leading_collapse_contrib == 0i32 && proc.leading_collapse_contrib != 0i32 {
                leading_collapse_contrib = proc.leading_collapse_contrib;
            }
        }
        (
            reflowed_count,
            y_cursor,
            previous_bottom_margin,
            last_placed_info,
            leading_collapse_contrib,
        )
    }

    #[inline]
    /// Per-child processing for `place_block_children_loop` to keep the loop small and readable.
    fn process_one_child(
        &mut self,
        loop_ctx: &PlaceLoopCtx<'_>,
        inputs: &ProcessChildIn,
    ) -> ProcessChildOut {
        // Determine if the child establishes a new block formatting context (BFC).
        // If so, external floats must not affect it: ignore external float floors and bands.
        let style = self
            .computed_styles
            .get(&inputs.child_key)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let child_is_bfc = establishes_block_formatting_context(&style);
        // Compute clearance floor first (clear may lift the child below floats). For BFC children, mask floors.
        let masked_left_for_child = if child_is_bfc {
            0i32
        } else {
            inputs.masked_left
        };
        let masked_right_for_child = if child_is_bfc {
            0i32
        } else {
            inputs.masked_right
        };
        let clearance_floor_y = self.compute_clearance_floor_for_child(
            inputs.child_key,
            masked_left_for_child,
            masked_right_for_child,
        );
        // Compute bands at the relevant y. Align the query y with the actual child top margin edge.
        let (y_for_bands, collapsed_top, margin_top_eff) =
            self.band_query_y_for_child(loop_ctx, inputs, &style, clearance_floor_y);
        let (band_left, band_right) =
            self.compute_float_bands_for_y(loop_ctx, inputs.index, y_for_bands);
        debug!(
            "[CHILD] idx={} key={:?} float={:?} clear={:?} bands=({}, {}) y_cursor={} y_for_bands={} collapsed_top={} mt_eff={}",
            inputs.index,
            inputs.child_key,
            style.float,
            style.clear,
            band_left,
            band_right,
            inputs.y_cursor,
            y_for_bands,
            collapsed_top,
            margin_top_eff
        );
        let ctx = Self::build_child_ctx(loop_ctx, inputs, clearance_floor_y, band_left, band_right);
        log::debug!(
            "[CTX] idx={} first={} parent_edge_collapsible={} y_cursor_in={} prev_bottom_in={} -> ctx.prev_bottom={} parent_self_top_margin={} leading_top_applied={} ancestor_applied_at_edge_for_children={}",
            inputs.index,
            inputs.index == loop_ctx.first_inflow_index,
            loop_ctx.parent_edge_collapsible,
            inputs.y_cursor,
            inputs.previous_bottom_margin,
            ctx.previous_bottom_margin,
            ctx.parent_self_top_margin,
            ctx.leading_top_applied,
            ctx.ancestor_applied_at_edge_for_children
        );
        let (y_calc, mb_calc, leading_contrib_raw) =
            self.layout_child_and_advance(loop_ctx.root, inputs.child_key, ctx);
        let (y_next, mb_next, last_info) = self.flow_result_after_layout(inputs, y_calc, mb_calc);
        // Update clearance floors by side if this child floats, using the unmasked running floors.
        let (left_floor_next, right_floor_next) = self.update_clearance_floors_for_float(
            inputs.child_key,
            inputs.current_left,
            inputs.current_right,
        );
        // Capture leading collapse contribution only for the first in-flow child.
        let leading_collapse_contrib = if inputs.index == loop_ctx.first_inflow_index {
            leading_contrib_raw
        } else {
            0i32
        };
        ProcessChildOut {
            y_next,
            mb_next,
            last_info,
            left_floor_next,
            right_floor_next,
            leading_collapse_contrib,
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
                ancestor_applied_at_edge_for_children: loop_ctx.ancestor_applied_at_edge,
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
            parent_origin_y: cb10::parent_content_origin(&loop_ctx.metrics).1,
            y_position: child_y,
            y_cursor_in: y_cursor,
            leading_top_applied: 0,
            child_key,
            rect: LayoutRect {
                x: i32::saturating_add(child_x, x_adjust) as f32,
                y: i32::saturating_add(child_y, y_adjust) as f32,
                width: used_bb_w as f32,
                height: 0.0,
            },
        });
    }

    /// Lay out a single block-level child and return `(height, y_position, margin_bottom, collapsed_top, clear_lifted)`.
    fn layout_one_block_child(&mut self, child_key: NodeKey, ctx: ChildLayoutCtx) -> PlacedBlock {
        orchestrator::place_child::place_child_public(self, child_key, ctx)
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
        chapter10::part_10_3_3_block_widths::compute_collapsed_and_position_public(
            self, child_key, ctx, style, sides,
        )
    }

    #[inline]
    /// Prepare the used width and initial position for a child without applying clearance adjustments.
    /// Used for committing zero-height leading boxes where we want width/x positioning, but do not
    /// affect the parent's vertical flow.
    fn prepare_child_position(
        style: &ComputedStyle,
        sides: &BoxSides,
        ctx: &ChildLayoutCtx,
        margin_top_eff: i32,
    ) -> (i32, i32, i32, i32, i32) {
        let collapsed_top =
            cm83::compute_collapsed_vertical_margin_public(ctx, margin_top_eff, style);
        let (parent_x, parent_y) = cb10::parent_content_origin(&ctx.metrics);
        let parent_right = parent_x.saturating_add(ctx.metrics.container_width);
        let (used_bb_w, child_x, _resolved_ml) =
            chapter10::part_10_3_3_block_widths::compute_horizontal_position_public(
                style,
                sides,
                parent_x,
                parent_right,
                (ctx.float_band_left, ctx.float_band_right),
            );
        let (x_adjust, y_adjust) =
            chapter9::part_9_4_3_relative_positioning::apply_relative_offsets(style);
        let child_y = cm83::compute_y_position_public(parent_y, ctx.y_cursor, collapsed_top);
        (used_bb_w, child_x, child_y, x_adjust, y_adjust)
    }

    #[inline]
    /// Compute heights and outgoing margins for a child by delegating to §10.6.3 composite.
    fn compute_heights_and_margins(&mut self, hctx: HeightsCtx<'_>) -> HeightsAndMargins {
        chapter10::part_10_6_3_height_of_blocks::compute_heights_and_margins_public(self, hctx)
    }

    #[inline]
    /// Build child metrics and compute raw content height by laying out descendants.
    /// Returns `(content_height, last_positive_bottom_margin)`.
    fn compute_child_content_height(&mut self, cctx: ChildContentCtx) -> (i32, i32) {
        chapter10::part_10_6_3_height_of_blocks::compute_child_content_height(self, cctx)
    }

    #[inline]
    /// Returns true if the node has any inline text descendant.
    ///
    /// Non-normative: helper for emptiness checks in vertical margin collapsing.
    fn has_inline_text_descendant(&self, key: NodeKey) -> bool {
        orchestrator::tree::has_inline_text_descendant(self, key)
    }

    #[inline]
    /// Choose the layout root. Prefer `body` under `html` when present; otherwise first block.
    ///
    /// Non-normative: document utility.
    fn choose_layout_root(&self) -> Option<NodeKey> {
        orchestrator::tree::choose_layout_root(self)
    }

    #[inline]
    /// Build the last-placed info tuple for diagnostics and content-bottom calculation.
    pub(crate) fn last_info_for_child(
        &self,
        child_key: NodeKey,
        mb_out: i32,
    ) -> Option<(NodeKey, i32, i32)> {
        orchestrator::diagnostics::last_info_for_child(self, child_key, mb_out)
    }

    #[inline]
    /// Construct `ContainerMetrics` for a child, given used width and edges.
    pub(crate) fn build_child_metrics(
        used_bb_w: i32,
        horiz: HorizontalEdges,
        top: TopEdges,
        x: i32,
        y: i32,
    ) -> ContainerMetrics {
        cb10::build_child_metrics(used_bb_w, horiz, top, x, y)
    }

    #[inline]
    /// Build the parent's `BoxSides` and decide if the top edge is collapsible per §8.3.1.
    fn build_parent_edge_context(
        &self,
        root: NodeKey,
        metrics: &ContainerMetrics,
    ) -> (BoxSides, bool) {
        let style = self
            .computed_styles
            .get(&root)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let sides = compute_box_sides(&style);
        let edge_collapsible = metrics.padding_top == 0i32
            && metrics.border_top == 0i32
            && !establishes_block_formatting_context(&style);
        (sides, edge_collapsible)
    }

    #[inline]
    /// Log initial context for the first placed child under a parent (diagnostics only).
    fn log_first_child_context(root: NodeKey, ctx: &ChildLayoutCtx) {
        orchestrator::diagnostics::log_first_child_context(root, ctx);
    }

    #[inline]
    /// Lay out one block child and advance `y` cursor and outgoing bottom margin.
    fn layout_child_and_advance(
        &mut self,
        root: NodeKey,
        child_key: NodeKey,
        ctx: ChildLayoutCtx,
    ) -> (i32, i32, i32) {
        if ctx.is_first_placed {
            Self::log_first_child_context(root, &ctx);
        }
        let placed = self.layout_one_block_child(child_key, ctx);
        if ctx.is_first_placed && !placed.parent_edge_collapsible {
            let (_px, parent_y) = cb10::parent_content_origin(&ctx.metrics);
            log::debug!(
                "[FIRST-NONCOLL out] root={root:?} child={child_key:?} parent_y={parent_y} y_cursor={} y_out={} collapsed_top={} prev_bottom={} parent_self_top_margin={} clear_lifted={}",
                ctx.y_cursor,
                placed.y,
                placed.collapsed_top,
                ctx.previous_bottom_margin,
                ctx.parent_self_top_margin,
                placed.clear_lifted
            );
        }
        if ctx.is_first_placed {
            log::debug!(
                "[FIRST-INFLOW DIAG] parent_edge_collapsible={} pre_clear_y={} collapsed_top={} leading_contrib={} clear_lifted={} y_out={}",
                placed.parent_edge_collapsible,
                ctx.y_cursor,
                placed.collapsed_top,
                placed.leading_collapse_contrib,
                placed.clear_lifted,
                placed.y
            );
        }
        // Advance cursor. If clearance lifted the child above the collapsed-top pre-position,
        // use the absolute placed y; otherwise advance in parent-relative space.
        let y_next = if placed.clear_lifted {
            placed.y.saturating_add(placed.content_height)
        } else {
            ctx.y_cursor
                .saturating_add(placed.collapsed_top)
                .saturating_add(placed.content_height)
        };
        // Propagate the signed outgoing bottom margin to the next sibling to allow
        // proper collapsing with the next child's top margin per §8.3.1.
        let mb_next = placed.outgoing_bottom_margin;
        (y_next, mb_next, placed.leading_collapse_contrib)
    }

    #[inline]
    /// Decide whether a box is effectively empty for vertical margin collapsing purposes.
    fn is_effectively_empty_box(
        &self,
        style: &ComputedStyle,
        sides: &BoxSides,
        computed_h: i32,
        child_key: NodeKey,
    ) -> bool {
        computed_h == 0
            && sides.padding_top == 0
            && sides.padding_bottom == 0
            && sides.border_top == 0
            && sides.border_bottom == 0
            && !self.has_inline_text_descendant(child_key)
            && !establishes_block_formatting_context(style)
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
        chapter9::part_9_5_floats::compute_clearance_floor_for_child(
            self,
            child_key,
            floor_left,
            floor_right,
        )
    }
    /// Emit a vertical log and insert the child's rect.
    pub(crate) fn commit_vert(&mut self, vert_commit: VertCommit) {
        orchestrator::diagnostics::log_vert(VertLog {
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
            } else {
                debug!("[LAYOUT][DIAG] insert_rect key={key:?} rect=({x}, {y}, {width}, {height})");
            }
        } else {
            debug!("[LAYOUT][DIAG] insert_rect key={key:?} rect=({x}, {y}, {width}, {height})");
        }
        orchestrator::diagnostics::insert_child_rect(&mut self.rects, key, rect);
    }

    // duplicate commit_vert removed

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
        chapter9::part_9_5_floats::update_clearance_floors_for_float(
            self,
            child_key,
            current_left,
            current_right,
        )
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
        chapter9::part_9_5_floats::compute_float_bands_for_y(
            self,
            loop_ctx,
            up_to_index,
            y_in_parent,
        )
    }

    #[inline]
    /// Decide the resulting `cursor/margin/last_info` after laying out a child. Floats are out-of-flow.
    fn flow_result_after_layout(
        &self,
        inputs: &ProcessChildIn,
        y_calc: i32,
        mb_calc: i32,
    ) -> (i32, i32, LastPlacedInfo) {
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

    #[inline]
    /// Build child context for a child.
    const fn build_child_ctx(
        loop_ctx: &PlaceLoopCtx<'_>,
        inputs: &ProcessChildIn,
        clearance_floor_y: i32,
        band_left: i32,
        band_right: i32,
    ) -> ChildLayoutCtx {
        ChildLayoutCtx {
            index: inputs.index,
            is_first_placed: inputs.index == loop_ctx.first_inflow_index,
            metrics: loop_ctx.metrics,
            y_cursor: inputs.y_cursor,
            previous_bottom_margin: if inputs.index == loop_ctx.first_inflow_index {
                if loop_ctx.parent_edge_collapsible {
                    loop_ctx.prev_bottom_after
                } else {
                    0
                }
            } else {
                inputs.previous_bottom_margin
            },
            parent_self_top_margin: if loop_ctx.parent_edge_collapsible
                && inputs.index == loop_ctx.first_inflow_index
                && !loop_ctx.ancestor_applied_at_edge
            {
                loop_ctx.parent_sides.margin_top
            } else {
                0
            },
            leading_top_applied: if loop_ctx.parent_edge_collapsible
                && inputs.index == loop_ctx.first_inflow_index
            {
                loop_ctx.leading_applied
            } else {
                0i32
            },
            ancestor_applied_at_edge_for_children: loop_ctx.ancestor_applied_at_edge,
            parent_edge_collapsible: loop_ctx.parent_edge_collapsible,
            clearance_floor_y,
            float_band_left: band_left,
            float_band_right: band_right,
        }
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
    /// Compute layout and return a snapshot of all rectangles.
    ///
    /// Compatibility shim for legacy callers expecting geometry after layout.
    pub fn compute_layout_geometry(&mut self) -> HashMap<NodeKey, LayoutRect> {
        self.compute_layout();
        self.rects.clone()
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
    /// Take and clear the accumulated dirty rectangles since the last query.
    pub fn take_dirty_rects(&mut self) -> Vec<LayoutRect> {
        take(&mut self.dirty_rects)
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
