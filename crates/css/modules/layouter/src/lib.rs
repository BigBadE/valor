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
use css_box::{BoxSides, compute_box_sides};
use css_text::default_line_height_px;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use log::debug;
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

/// Bundle for committing vertical results and rectangle for a child.
#[derive(Clone, Copy)]
struct VertCommit {
    /// Child index within parent's block children.
    index: usize,
    /// Previous sibling bottom margin (pre-collapsed).
    prev_mb: i32,
    /// Raw top margin from computed sides.
    margin_top_raw: i32,
    /// Effective top margin after collapsing through empties.
    margin_top_eff: i32,
    /// Effective bottom margin after collapsing through empties.
    eff_bottom: i32,
    /// Whether the child is effectively empty for collapsing.
    is_empty: bool,
    /// Collapsed top offset applied at this edge.
    collapsed_top: i32,
    /// Parent content origin y.
    parent_origin_y: i32,
    /// Final y position for the child.
    y_position: i32,
    /// Incoming y cursor in parent content space.
    y_cursor_in: i32,
    /// Leading-top collapse applied at parent edge for the first child, if any.
    leading_top_applied: i32,
    /// The child node key.
    child_key: NodeKey,
    /// Final border-box rectangle for the child.
    rect: LayoutRect,
}

/// Context for computing a child's content height by laying out its descendants.
#[derive(Clone, Copy)]
struct ChildContentCtx {
    /// The child node key whose descendants will be laid out.
    key: NodeKey,
    /// Child used border-box width.
    used_border_box_width: i32,
    /// Box sides (padding/border/margins) snapshot for the child.
    sides: BoxSides,
    /// Child x position (margin edge).
    x: i32,
    /// Child y position (margin edge).
    y: i32,
}

/// Inputs captured for vertical layout logs.
#[derive(Clone, Copy)]
struct VertLog {
    /// Index of the child within the parent block children.
    index: usize,
    /// Previous sibling's bottom margin (pre-collapsed).
    prev_mb: i32,
    /// Child raw top margin from computed sides.
    margin_top_raw: i32,
    /// Child effective top margin used for collapse with parent/previous.
    margin_top_eff: i32,
    /// Child effective bottom margin (post internal propagation through empties).
    eff_bottom: i32,
    /// Whether the child is considered empty for vertical collapsing.
    is_empty: bool,
    /// Result of top margin collapsing applied at this edge.
    collapsed_top: i32,
    /// Parent content origin y.
    parent_origin_y: i32,
    /// Final chosen y position for the child.
    y_position: i32,
    /// Incoming y cursor in the parent content space.
    y_cursor_in: i32,
    /// Leading-top collapse applied at parent edge (if any) for first child.
    leading_top_applied: i32,
}

/// Tuple of optional width constraints (specified, min, max) in border-box space.
type WidthConstraints = (Option<i32>, Option<i32>, Option<i32>);

/// Inputs captured for horizontal solving logs.
#[derive(Clone, Copy)]
struct HorizInputs {
    /// Container content width.
    container_w: i32,
    /// Box sides used (padding/border) for diagnostics.
    sides: BoxSides,
    /// Original author-specified margin-left value (may be negative).
    in_margin_left: i32,
    /// Original author-specified margin-right value (may be negative).
    in_margin_right: i32,
    /// Whether margin-left was 'auto'.
    left_auto: bool,
    /// Whether margin-right was 'auto'.
    right_auto: bool,
}

/// Resolution context for the specified-width horizontal solving path.
#[derive(Clone, Copy)]
struct ConstrainedHorizCtx {
    /// Final constrained border-box width.
    constrained_bb: i32,
    /// Whether margin-left is auto.
    left_auto: bool,
    /// Whether margin-right is auto.
    right_auto: bool,
    /// Container content width.
    container_content_width: i32,
    /// Current margin-left value.
    margin_left_resolved: i32,
    /// Current margin-right value.
    margin_right_resolved: i32,
}

/// Context for the width:auto horizontal solving path.
#[derive(Clone, Copy)]
struct AutoHorizCtx {
    /// Whether margin-left is auto.
    left_auto: bool,
    /// Whether margin-right is auto.
    right_auto: bool,
    /// Container content width.
    container_content_width: i32,
    /// Current margin-left value.
    margin_left_resolved: i32,
    /// Current margin-right value.
    margin_right_resolved: i32,
    /// Minimum border-box width constraint (if any).
    min_bb_opt: Option<i32>,
    /// Maximum border-box width constraint (if any).
    max_bb_opt: Option<i32>,
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

/// Vertical padding and border widths for height calculations.
#[derive(Clone, Copy)]
struct HeightExtras {
    /// Child padding-top in pixels.
    padding_top: i32,
    /// Child padding-bottom in pixels.
    padding_bottom: i32,
    /// Child border-top in pixels.
    border_top: i32,
    /// Child border-bottom in pixels.
    border_bottom: i32,
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
    /// Parent's own top margin to include when collapsing with the first child's top.
    parent_self_top_margin: i32,
    /// Leading top collapse applied at parent edge (from a leading empty chain), if any.
    leading_top_applied: i32,
}

#[inline]
/// Compute and apply the leading-empty-chain collapse per CSS §8.3.1.
/// Returns (`y_cursor_start`, `previous_bottom_margin`, `leading_top_applied`, `skip_count`).
fn apply_leading_top_collapse(
    layouter: &Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
    block_children: &[NodeKey],
) -> (i32, i32, i32, usize) {
    if block_children.is_empty() {
        debug!("[VERT-GROUP root={root:?}] skip pre-scan: empty children");
        return (0i32, 0i32, 0i32, 0usize);
    }

    let parent_style = layouter
        .computed_styles
        .get(&root)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let parent_sides = compute_box_sides(&parent_style);
    let include_parent_edge = metrics.padding_top == 0i32 && metrics.border_top == 0i32;
    // If the parent's top edge is collapsible, include the parent's own top margin.
    // Otherwise, compute an internal leading collapse (without the parent's top margin).
    let mut leading_margins: Vec<i32> = if include_parent_edge {
        vec![parent_sides.margin_top]
    } else {
        Vec::new()
    };
    let mut skip_count: usize = 0;
    let mut idx: usize = 0;
    while let Some(child_key) = block_children.get(idx).copied() {
        let child_style = layouter
            .computed_styles
            .get(&child_key)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let child_sides = compute_box_sides(&child_style);
        let eff_top = layouter.effective_child_top_margin(child_key, &child_sides);
        let is_leading_empty = layouter.is_structurally_empty_chain(child_key);
        debug!(
            "[VERT-GROUP scan root={root:?}] child={child_key:?} eff_top={eff_top} paddings(top={},bottom={}) borders(top={},bottom={}) height={:?} structurally_empty_chain={}",
            child_sides.padding_top,
            child_sides.padding_bottom,
            child_sides.border_top,
            child_sides.border_bottom,
            child_style.height,
            is_leading_empty
        );
        if is_leading_empty {
            let eff_bottom = layouter.effective_child_bottom_margin(child_key, &child_sides);
            leading_margins.push(eff_top);
            leading_margins.push(eff_bottom);
            skip_count = skip_count.saturating_add(1);
            idx = idx.saturating_add(1);
            continue;
        }
        leading_margins.push(eff_top);
        break;
    }
    if skip_count == 0 && leading_margins.is_empty() {
        return (0i32, 0i32, 0i32, 0usize);
    }
    let leading_top = Layouter::collapse_margins_list(&leading_margins);
    debug!(
        "[VERT-GROUP root={root:?}] include_parent_edge={include_parent_edge} leading_skip={skip_count} margins={leading_margins:?} -> leading_top={leading_top}"
    );
    if include_parent_edge {
        // Apply at the parent's top edge; do not pass it down as previous_bottom_margin.
        (leading_top.max(0i32), 0i32, leading_top, skip_count)
    } else {
        // Parent top edge is blocked by padding/border. Keep y at 0 and pass the internal
        // collapsed leading as previous_bottom_margin to the first child. Do not mark as applied.
        (0i32, leading_top, 0i32, skip_count)
    }
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
    /// Solve used border-box width and horizontal margins together for a non-replaced block in normal flow.
    /// Implements CSS 2.2 §10.3.3 for horizontal dimensions.
    fn solve_block_horizontal(
        style: &ComputedStyle,
        sides: &BoxSides,
        container_content_width: i32,
        margin_left_in: i32,
        margin_right_in: i32,
    ) -> (i32, i32, i32) {
        let (specified_bb_opt, min_bb_opt, max_bb_opt) =
            Self::compute_width_constraints(style, sides);
        let left_auto = style.margin_left_auto;
        let right_auto = style.margin_right_auto;
        let margin_left_resolved = margin_left_in;
        let margin_right_resolved = margin_right_in;

        if specified_bb_opt.is_some() {
            let constrained = used_border_box_width(style, container_content_width);
            let ctx = ConstrainedHorizCtx {
                constrained_bb: constrained,
                left_auto,
                right_auto,
                container_content_width,
                margin_left_resolved,
                margin_right_resolved,
            };
            let out = Self::resolve_with_constrained_width(ctx);
            let inputs = HorizInputs {
                container_w: container_content_width,
                sides: *sides,
                in_margin_left: margin_left_in,
                in_margin_right: margin_right_in,
                left_auto,
                right_auto,
            };
            Self::log_horiz("specified", &inputs, constrained, out);
            return out;
        }

        let ctx = AutoHorizCtx {
            left_auto,
            right_auto,
            container_content_width,
            margin_left_resolved,
            margin_right_resolved,
            min_bb_opt,
            max_bb_opt,
        };
        let out = Self::resolve_auto_width(ctx);
        let inputs = HorizInputs {
            container_w: container_content_width,
            sides: *sides,
            in_margin_left: margin_left_in,
            in_margin_right: margin_right_in,
            left_auto,
            right_auto,
        };
        Self::log_horiz("auto", &inputs, out.0, out);
        out
    }

    #[inline]
    /// Clamp a width value in border-box space using optional min/max constraints.
    fn clamp_width_to_min_max(
        width_value: i32,
        min_bb_opt: Option<i32>,
        max_bb_opt: Option<i32>,
    ) -> i32 {
        let mut out = width_value;
        if let Some(min_b) = min_bb_opt {
            out = out.max(min_b);
        }
        if let Some(max_b) = max_bb_opt {
            out = out.min(max_b);
        }
        out
    }

    #[inline]
    /// Compute width constraints converted to border-box space based on the element's box-sizing.
    fn compute_width_constraints(style: &ComputedStyle, sides: &BoxSides) -> WidthConstraints {
        let extras = sides
            .padding_left
            .saturating_add(sides.padding_right)
            .saturating_add(sides.border_left)
            .saturating_add(sides.border_right);
        let specified_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .width
                .map(|width_val| (width_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.width.map(|width_val| width_val as i32),
        };
        let min_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .min_width
                .map(|min_val| (min_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.min_width.map(|min_val| min_val as i32),
        };
        let max_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .max_width
                .map(|max_val| (max_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.max_width.map(|max_val| max_val as i32),
        };
        (specified_bb_opt, min_bb_opt, max_bb_opt)
    }

    #[inline]
    /// Compute `lhs - rhs` allowing negative results using saturating ops.
    const fn diff_i32(lhs: i32, rhs: i32) -> i32 {
        if lhs >= rhs {
            lhs.saturating_sub(rhs)
        } else {
            let delta = rhs.saturating_sub(lhs);
            0i32.saturating_sub(delta)
        }
    }

    #[inline]
    /// Log a single horizontal solve step.
    fn log_horiz(path: &str, inputs: &HorizInputs, width_in: i32, out: (i32, i32, i32)) {
        debug!(
            "[HORIZ {path}] cont_w={} extras(pl,pr,bl,br)=({},{},{},{}) in(ml={},mr={}) auto(l={},r={}) width_in={} -> out(width={}, ml={}, mr={})",
            inputs.container_w,
            inputs.sides.padding_left,
            inputs.sides.padding_right,
            inputs.sides.border_left,
            inputs.sides.border_right,
            inputs.in_margin_left,
            inputs.in_margin_right,
            inputs.left_auto,
            inputs.right_auto,
            width_in,
            out.0,
            out.1,
            out.2,
        );
    }

    /// Resolve margins given a constrained border-box width (specified width path).
    fn resolve_with_constrained_width(mut ctx: ConstrainedHorizCtx) -> (i32, i32, i32) {
        let constrained = ctx.constrained_bb.max(0i32);
        if ctx.left_auto && ctx.right_auto {
            let remaining = Self::diff_i32(ctx.container_content_width, constrained);
            let abs_remaining = if remaining >= 0i32 {
                remaining
            } else {
                0i32.saturating_sub(remaining)
            };
            let half = abs_remaining >> 1i32;
            if remaining >= 0i32 {
                ctx.margin_left_resolved = half;
                ctx.margin_right_resolved = abs_remaining.saturating_sub(half);
            } else {
                ctx.margin_left_resolved = 0i32.saturating_sub(half);
                ctx.margin_right_resolved = 0i32.saturating_sub(abs_remaining.saturating_sub(half));
            }
            return (
                constrained,
                ctx.margin_left_resolved,
                ctx.margin_right_resolved,
            );
        }
        if ctx.left_auto ^ ctx.right_auto {
            if ctx.left_auto {
                ctx.margin_left_resolved = Self::diff_i32(
                    Self::diff_i32(ctx.container_content_width, constrained),
                    ctx.margin_right_resolved,
                );
            } else {
                ctx.margin_right_resolved = Self::diff_i32(
                    Self::diff_i32(ctx.container_content_width, constrained),
                    ctx.margin_left_resolved,
                );
            }
            return (
                constrained,
                ctx.margin_left_resolved,
                ctx.margin_right_resolved,
            );
        }
        // Over-constrained: adjust margin-right (assuming LTR; no direction support in shim yet).
        ctx.margin_right_resolved = Self::diff_i32(
            Self::diff_i32(ctx.container_content_width, constrained),
            ctx.margin_left_resolved,
        );
        (
            constrained,
            ctx.margin_left_resolved,
            ctx.margin_right_resolved,
        )
    }

    /// Resolve margins and compute border-box width for the width:auto path.
    fn resolve_auto_width(mut ctx: AutoHorizCtx) -> (i32, i32, i32) {
        let mut border_box_auto = if ctx.left_auto && ctx.right_auto {
            ctx.margin_left_resolved = 0i32;
            ctx.margin_right_resolved = 0i32;
            ctx.container_content_width
        } else if ctx.left_auto ^ ctx.right_auto {
            if ctx.left_auto {
                ctx.margin_left_resolved = 0i32;
                Self::diff_i32(ctx.container_content_width, ctx.margin_right_resolved)
            } else {
                ctx.margin_right_resolved = 0i32;
                Self::diff_i32(ctx.container_content_width, ctx.margin_left_resolved)
            }
        } else {
            let tmp = Self::diff_i32(ctx.container_content_width, ctx.margin_left_resolved);
            Self::diff_i32(tmp, ctx.margin_right_resolved)
        };
        border_box_auto =
            Self::clamp_width_to_min_max(border_box_auto, ctx.min_bb_opt, ctx.max_bb_opt);
        (
            border_box_auto.max(0i32),
            ctx.margin_left_resolved,
            ctx.margin_right_resolved,
        )
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
    /// Find the last block-level node in depth-first order under `start`.
    fn find_last_block_under(&self, start: NodeKey) -> Option<NodeKey> {
        let mut last: Option<NodeKey> = None;
        if let Some(child_list) = self.children.get(&start) {
            for child_key in child_list {
                if let Some(found) = self.find_last_block_under(*child_key) {
                    last = Some(found);
                } else if matches!(
                    self.nodes.get(child_key),
                    Some(&LayoutNodeKind::Block { .. })
                ) {
                    last = Some(*child_key);
                }
            }
        }
        if last.is_none() && matches!(self.nodes.get(&start), Some(&LayoutNodeKind::Block { .. })) {
            last = Some(start);
        }
        last
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
        let (_content_top, content_bottom) = self.aggregate_content_extents(root);
        // Keep the parent's y derived from top-margin collapse; do not override with child top.
        let root_y_aligned = root_y;
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
            let first_sides = compute_box_sides(&first_style);
            let first_effective_top = self.effective_child_top_margin(first_child, &first_sides);
            let collapsed = Self::collapse_margins_pair(metrics.margin_top, first_effective_top);
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
            // Consider only element (block) children for block layout ordering.
            let mut block_children: Vec<NodeKey> = Vec::new();
            for key in child_list {
                if matches!(self.nodes.get(&key), Some(LayoutNodeKind::Block { .. })) {
                    block_children.push(key);
                }
            }
            let (y_start, prev_bottom_after, leading_applied, skipped) =
                apply_leading_top_collapse(self, root, metrics, &block_children);
            debug!(
                "[VERT-GROUP apply root={root:?}] y_start={y_start} prev_bottom_after={prev_bottom_after} leading_applied={leading_applied}"
            );
            y_cursor = y_start;
            let mut previous_bottom_margin: i32 = prev_bottom_after;
            // Parent's own top margin is needed for first-child 'first edge' incremental calculation
            let parent_style = self
                .computed_styles
                .get(&root)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let parent_sides = compute_box_sides(&parent_style);
            let parent_edge_collapsible = metrics.padding_top == 0i32 && metrics.border_top == 0i32;
            for (index, child_key) in block_children.into_iter().enumerate() {
                let ctx = ChildLayoutCtx {
                    index,
                    metrics: *metrics,
                    y_cursor,
                    previous_bottom_margin,
                    parent_self_top_margin: if parent_edge_collapsible {
                        parent_sides.margin_top
                    } else {
                        0
                    },
                    leading_top_applied: if index == skipped {
                        leading_applied
                    } else {
                        0i32
                    },
                };
                if index == 0 {
                    Self::log_first_child_context(root, &ctx);
                }
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
        let margin_top_eff = self.effective_child_top_margin(child_key, &sides);
        let collapsed_top = Self::compute_collapsed_vertical_margin(&ctx, margin_top_eff);
        let (parent_x, parent_y) = Self::parent_content_origin(&ctx.metrics);
        let (used_bb_w, resolved_ml, _resolved_mr) = Self::solve_block_horizontal(
            &style,
            &sides,
            ctx.metrics.container_width,
            sides.margin_left,
            sides.margin_right,
        );
        let (x_adjust, y_adjust) = Self::apply_relative_offsets(&style);
        let child_x = parent_x.saturating_add(resolved_ml);
        let child_y = Self::compute_y_position(parent_y, ctx.y_cursor, collapsed_top);
        let (mut content_h, last_pos_mb) = self.compute_child_content_height(ChildContentCtx {
            key: child_key,
            used_border_box_width: used_bb_w,
            sides,
            x: child_x,
            y: child_y,
        });
        if (sides.padding_bottom > 0i32 || sides.border_bottom > 0i32) && last_pos_mb > 0i32 {
            content_h = content_h.saturating_add(last_pos_mb);
        }
        let computed_h = Self::compute_used_height(
            self,
            &style,
            child_key,
            HeightExtras {
                padding_top: sides.padding_top,
                padding_bottom: sides.padding_bottom,
                border_top: sides.border_top,
                border_bottom: sides.border_bottom,
            },
            content_h,
        );
        let eff_bottom = self.effective_child_bottom_margin(child_key, &sides);
        let is_empty = self.is_effectively_empty_box(&style, &sides, computed_h, child_key);
        let margin_bottom_out = if is_empty && ctx.index == 0 {
            let list = [ctx.parent_self_top_margin, margin_top_eff, eff_bottom];
            Self::collapse_margins_list(&list)
        } else {
            Self::compute_margin_bottom_out(margin_top_eff, eff_bottom, is_empty)
        };

        if ctx.index == 0 {
            let edge_blocked = ctx.metrics.padding_top != 0 || ctx.metrics.border_top != 0;
            debug!(
                "[VERT-COLLAPSE summary child={:?} idx=0] edge_blocked={} lt_applied={} prev_mb={} parent_top={} child_top_eff={} collapsed_top={} -> y={}",
                child_key,
                edge_blocked,
                ctx.leading_top_applied,
                ctx.previous_bottom_margin,
                ctx.parent_self_top_margin,
                margin_top_eff,
                collapsed_top,
                child_y
            );
        }

        self.commit_vert(VertCommit {
            index: ctx.index,
            prev_mb: ctx.previous_bottom_margin,
            margin_top_raw: sides.margin_top,
            margin_top_eff,
            eff_bottom,
            is_empty,
            collapsed_top,
            parent_origin_y: parent_y,
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
    /// Build child container metrics and compute raw content height by laying out descendants.
    /// Returns `(content_height, last_positive_bottom_margin)`.
    fn compute_child_content_height(&mut self, cctx: ChildContentCtx) -> (i32, i32) {
        let child_metrics = Self::build_child_metrics(
            cctx.used_border_box_width,
            HorizontalEdges {
                padding_left: cctx.sides.padding_left,
                padding_right: cctx.sides.padding_right,
                border_left: cctx.sides.border_left,
                border_right: cctx.sides.border_right,
            },
            TopEdges {
                padding_top: cctx.sides.padding_top,
                border_top: cctx.sides.border_top,
            },
            cctx.x,
            cctx.y,
        );
        let (_reflowed, content_height, last_pos_mb) =
            self.layout_block_children(cctx.key, &child_metrics);
        (content_height, last_pos_mb)
    }

    #[inline]
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
        Self::insert_child_rect(&mut self.rects, vert_commit.child_key, vert_commit.rect);
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
    /// Compute final outgoing bottom margin for a child, allowing internal top/bottom collapse when the child is empty.
    fn compute_margin_bottom_out(margin_top: i32, effective_bottom: i32, is_empty: bool) -> i32 {
        if is_empty {
            Self::collapse_margins_pair(margin_top, effective_bottom)
        } else {
            effective_bottom
        }
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
    /// Compute an effective top margin for a child, collapsing with its first block child's top margin
    /// when the child has no top padding/border and contains no inline text (approximation of CSS 2.2 §8.3.1).
    fn effective_child_top_margin(&self, child_key: NodeKey, child_sides: &BoxSides) -> i32 {
        let mut margins: Vec<i32> = vec![child_sides.margin_top];
        // Walk down first-block descendants while current box is eligible to pass-through top margin.
        let mut current = child_key;
        let mut current_sides = *child_sides;
        while current_sides.padding_top == 0i32
            && current_sides.border_top == 0i32
            && !self.has_inline_text_descendant(current)
            && let Some(first_desc) = self.find_first_block_under(current)
            && first_desc != current
        {
            let first_style = self
                .computed_styles
                .get(&first_desc)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let first_sides = compute_box_sides(&first_style);
            margins.push(first_sides.margin_top);
            current = first_desc;
            current_sides = first_sides;
        }
        Self::collapse_margins_list(&margins)
    }

    #[inline]
    /// Compute an effective bottom margin for a child, collapsing with its last block child's bottom margin
    /// when the child has no bottom padding/border and contains no inline text (approximation of CSS 2.2 §8.3.1).
    fn effective_child_bottom_margin(&self, child_key: NodeKey, child_sides: &BoxSides) -> i32 {
        let mut margins: Vec<i32> = vec![child_sides.margin_bottom];
        // Walk down last-block descendants while current box is eligible to pass-through bottom margin.
        let mut current = child_key;
        let mut current_sides = *child_sides;
        while current_sides.padding_bottom == 0i32
            && current_sides.border_bottom == 0i32
            && !self.has_inline_text_descendant(current)
            && let Some(last_desc) = self.find_last_block_under(current)
            && last_desc != current
        {
            let last_style = self
                .computed_styles
                .get(&last_desc)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let last_sides = compute_box_sides(&last_style);
            margins.push(last_sides.margin_bottom);
            current = last_desc;
            current_sides = last_sides;
        }
        Self::collapse_margins_list(&margins)
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
    fn compute_y_position(origin_y: i32, cursor: i32, collapsed_vertical_margin: i32) -> i32 {
        origin_y
            .saturating_add(cursor)
            .saturating_add(collapsed_vertical_margin.max(0i32))
    }

    #[inline]
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
    /// Return the first block child of `key`, if any.
    fn first_block_child(&self, key: NodeKey) -> Option<NodeKey> {
        let kids = self.children.get(&key)?;
        kids.iter()
            .copied()
            .find(|node_key| matches!(self.nodes.get(node_key), Some(LayoutNodeKind::Block { .. })))
    }

    #[inline]
    /// Heuristic structural emptiness used during leading group pre-scan (CSS §8.3.1).
    /// Walks a chain of first block children while each box has zero top/bottom padding and border.
    /// If the chain terminates without another block child under those constraints, treat as empty.
    fn is_structurally_empty_chain(&self, start: NodeKey) -> bool {
        let mut current = start;
        loop {
            let style = self
                .computed_styles
                .get(&current)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let sides = compute_box_sides(&style);
            if sides.padding_top != 0
                || sides.border_top != 0
                || sides.padding_bottom != 0
                || sides.border_bottom != 0
            {
                return false;
            }
            match self.first_block_child(current) {
                None => return true,
                Some(next) => {
                    current = next;
                }
            }
        }
    }

    #[inline]
    /// Compute the vertical offset from collapsed margins above a block child.
    fn compute_collapsed_vertical_margin(ctx: &ChildLayoutCtx, child_margin_top: i32) -> i32 {
        if ctx.index == 0 {
            if ctx.leading_top_applied != 0i32 {
                // The leading-top collapse was already applied at the parent edge.
                debug!(
                    "[VERT-COLLAPSE first] lt_applied={} -> collapsed_top=0",
                    ctx.leading_top_applied
                );
                return 0i32;
            }
            if ctx.metrics.padding_top == 0i32 && ctx.metrics.border_top == 0i32 {
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

    #[inline]
    /// Compute used height for a block child, applying box extras when height is auto
    /// and falling back to a single line height if there is inline text and overall height is 0.
    fn compute_used_height(
        &self,
        style: &ComputedStyle,
        child_key: NodeKey,
        extras: HeightExtras,
        child_content_height: i32,
    ) -> i32 {
        let mut computed_height = used_border_box_height(style);
        if style.height.is_none() {
            computed_height = child_content_height
                .saturating_add(extras.padding_top)
                .saturating_add(extras.padding_bottom)
                .saturating_add(extras.border_top)
                .saturating_add(extras.border_bottom);
            if computed_height == 0i32 && self.has_inline_text_descendant(child_key) {
                computed_height = default_line_height_px(style);
            }
        }
        computed_height
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
