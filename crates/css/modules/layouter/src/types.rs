use css_box::BoxSides;
use js::NodeKey;
use style_engine::ComputedStyle;

/// A rectangle in device-independent pixels (border-box space).
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    /// X coordinate of the border-box origin.
    pub x: i32,
    /// Y coordinate of the border-box origin.
    pub y: i32,
    /// Border-box width.
    pub width: i32,
    /// Border-box height.
    pub height: i32,
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
}

/// Inputs for computing a child's heights and outgoing bottom margin.
#[derive(Clone, Copy)]
pub struct HeightsCtx<'heights> {
    /// The child node key whose heights are being computed.
    pub child_key: NodeKey,
    /// The computed style of the child.
    pub style: &'heights ComputedStyle,
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
}

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
