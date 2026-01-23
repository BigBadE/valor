/// Float layout module implementing CSS 2.2 float positioning.
///
/// This module handles:
/// - Float positioning (float: left, right, none)
/// - Float stacking and wrapping
/// - Clearance (clear: left, right, both, none)
/// - Float interaction with BFC
/// - Exclusion zones for text wrapping (when inline layout is implemented)
///
/// Spec: https://www.w3.org/TR/CSS22/visuren.html#floats
use crate::{BlockMarker, ConstrainedMarker, InlineMarker, SizeQuery, Subpixels};
use rewrite_core::{NodeId, ScopedDb};
use rewrite_css::{ClearQuery, CssKeyword, CssValue, FloatQuery};

/// Represents a floating box with its position and size.
#[derive(Debug, Clone, Copy)]
pub struct FloatBox {
    /// The node ID of the floating element.
    pub node: NodeId,
    /// Block-axis offset (top edge).
    pub block_offset: Subpixels,
    /// Inline-axis offset (left or right edge).
    pub inline_offset: Subpixels,
    /// Block-axis size (height).
    pub block_size: Subpixels,
    /// Inline-axis size (width).
    pub inline_size: Subpixels,
    /// Float direction (left or right).
    pub direction: FloatDirection,
}

/// Float direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatDirection {
    Left,
    Right,
}

impl FloatBox {
    /// Get the bottom edge of the float.
    pub fn block_end(&self) -> Subpixels {
        self.block_offset + self.block_size
    }

    /// Get the right edge of the float (for left floats).
    pub fn inline_end(&self) -> Subpixels {
        self.inline_offset + self.inline_size
    }
}

/// Compute the offset for a floated element.
///
/// Floated elements are removed from normal flow and positioned to the left or right
/// edge of their containing block, as far up as possible, while respecting:
/// - Previous floats on the same side
/// - Previous floats on the opposite side
/// - The containing block boundaries
///
/// # Float Positioning Rules (CSS 2.2 Section 9.5.1):
///
/// 1. A left float's left edge must not be to the left of the containing block's left edge
/// 2. A right float's right edge must not be to the right of the containing block's right edge
/// 3. A left float's left edge must be to the right of any earlier left floats' right edges
/// 4. A right float's right edge must be to the left of any earlier right floats' left edges
/// 5. A float's top edge must be at or below the bottom edge of all earlier floats
/// 6. A float must be placed as high as possible
/// 7. A left float must be placed as far left as possible (within constraints 1-6)
/// 8. A right float must be placed as far right as possible (within constraints 1-6)
pub fn compute_float_offset(
    scoped: &mut ScopedDb,
    axis: crate::Layouts,
    normal_flow_offset: Subpixels,
) -> Subpixels {
    let float_value = scoped.query::<FloatQuery>();

    let direction = match float_value {
        CssValue::Keyword(CssKeyword::Left) => FloatDirection::Left,
        CssValue::Keyword(CssKeyword::Right) => FloatDirection::Right,
        _ => return normal_flow_offset,
    };

    match axis {
        crate::Layouts::Block => compute_float_block_offset(scoped),
        crate::Layouts::Inline => compute_float_inline_offset(scoped, direction),
    }
}

/// Compute the block-axis offset for a float.
///
/// The float should be positioned as high as possible while staying below all
/// earlier floats and respecting the clear property.
fn compute_float_block_offset(scoped: &mut ScopedDb) -> Subpixels {
    use crate::helpers;

    // Get the parent's content box start position
    let parent_start = helpers::parent_start::<BlockMarker>(scoped);

    // Get all previous floats in the same BFC
    let prev_floats = collect_previous_floats(scoped);

    // Calculate clearance if the clear property is set
    let clearance = compute_clearance(scoped, &prev_floats);

    // Base position is the bottom of the previous in-flow element or parent start
    let base_offset = compute_base_block_offset(scoped, parent_start);

    // The float must be below all previous floats
    let below_floats = prev_floats
        .iter()
        .map(|f| f.block_end())
        .max()
        .unwrap_or(base_offset);

    // Take the maximum of: base offset, below floats, and clearance
    base_offset.max(below_floats).max(clearance)
}

/// Compute the inline-axis offset for a float.
///
/// Left floats are positioned at the left edge of the containing block,
/// to the right of any earlier left floats.
///
/// Right floats are positioned at the right edge of the containing block,
/// to the left of any earlier right floats.
fn compute_float_inline_offset(scoped: &mut ScopedDb, direction: FloatDirection) -> Subpixels {
    use crate::helpers;

    let parent_start = helpers::parent_start::<InlineMarker>(scoped);
    let parent_size = scoped.parent::<SizeQuery<InlineMarker, ConstrainedMarker>>();
    let node_size = scoped.query::<SizeQuery<InlineMarker, ConstrainedMarker>>();

    // Get previous floats at the same block position
    let prev_floats = collect_previous_floats(scoped);
    let current_block_offset = scoped.query::<crate::OffsetQuery<BlockMarker>>();

    // Filter floats that overlap vertically with this float
    let overlapping_floats: Vec<FloatBox> = prev_floats
        .into_iter()
        .filter(|f| {
            let node_block_size = scoped.query::<SizeQuery<BlockMarker, ConstrainedMarker>>();
            let current_end = current_block_offset + node_block_size;
            let float_end = f.block_end();

            // Check for vertical overlap
            (current_block_offset < float_end) && (current_end > f.block_offset)
        })
        .collect();

    match direction {
        FloatDirection::Left => {
            // Position to the right of all earlier left floats
            let right_of_left_floats = overlapping_floats
                .iter()
                .filter(|f| f.direction == FloatDirection::Left)
                .map(|f| f.inline_end())
                .max()
                .unwrap_or(parent_start);

            right_of_left_floats
        }
        FloatDirection::Right => {
            // Position to the left of all earlier right floats
            let left_of_right_floats = overlapping_floats
                .iter()
                .filter(|f| f.direction == FloatDirection::Right)
                .map(|f| f.inline_offset)
                .min()
                .unwrap_or(parent_start + parent_size);

            left_of_right_floats - node_size
        }
    }
}

/// Compute the base block offset (without considering floats or clearance).
///
/// This is the position where the float would be in normal flow.
fn compute_base_block_offset(scoped: &mut ScopedDb, parent_start: Subpixels) -> Subpixels {
    // Get the bottom edge of the previous in-flow element
    let prev_sibling = scoped.prev_sibling();

    if let Some(prev) = prev_sibling {
        // Check if previous sibling is floated
        let prev_float = scoped.node_query::<FloatQuery>(prev);
        if matches!(prev_float, CssValue::Keyword(CssKeyword::None)) {
            // Previous is in-flow, position below it
            let prev_offset = scoped.node_query::<crate::OffsetQuery<BlockMarker>>(prev);
            let prev_size = scoped.node_query::<SizeQuery<BlockMarker, ConstrainedMarker>>(prev);
            return prev_offset + prev_size;
        }
    }

    // No previous in-flow sibling, use parent start
    parent_start
}

/// Collect all previous floating boxes in the same BFC.
///
/// This includes:
/// - Floated previous siblings
/// - Floats from parent's previous siblings (if they extend into this container)
///
/// Returns floats in document order.
fn collect_previous_floats(scoped: &mut ScopedDb) -> Vec<FloatBox> {
    let mut floats = Vec::new();

    // Collect floated previous siblings
    let prev_siblings = scoped
        .db()
        .resolve_relationship(scoped.node(), rewrite_core::Relationship::PreviousSiblings);

    for &sibling in &prev_siblings {
        if is_floated(scoped, sibling) {
            if let Some(float_box) = create_float_box(scoped, sibling) {
                floats.push(float_box);
            }
        }
    }

    // TODO: Collect floats from ancestor contexts that extend into this container
    // This requires tracking float context across BFC boundaries

    floats
}

/// Check if a node is floated.
fn is_floated(scoped: &mut ScopedDb, node: NodeId) -> bool {
    let float_value = scoped.node_query::<FloatQuery>(node);
    !matches!(float_value, CssValue::Keyword(CssKeyword::None))
}

/// Create a FloatBox from a node.
fn create_float_box(scoped: &mut ScopedDb, node: NodeId) -> Option<FloatBox> {
    let float_value = scoped.node_query::<FloatQuery>(node);
    let direction = match float_value {
        CssValue::Keyword(CssKeyword::Left) => FloatDirection::Left,
        CssValue::Keyword(CssKeyword::Right) => FloatDirection::Right,
        _ => return None,
    };

    let block_offset = scoped.node_query::<crate::OffsetQuery<BlockMarker>>(node);
    let inline_offset = scoped.node_query::<crate::OffsetQuery<InlineMarker>>(node);
    let block_size = scoped.node_query::<SizeQuery<BlockMarker, ConstrainedMarker>>(node);
    let inline_size = scoped.node_query::<SizeQuery<InlineMarker, ConstrainedMarker>>(node);

    Some(FloatBox {
        node,
        block_offset,
        inline_offset,
        block_size,
        inline_size,
        direction,
    })
}

// ============================================================================
// Clearance
// ============================================================================

/// Compute clearance for an element with the 'clear' property.
///
/// Clearance is additional space added above an element to push it below floats.
///
/// # Clear Property Values:
/// - `none`: No clearance (default)
/// - `left`: Clear left floats
/// - `right`: Clear right floats
/// - `both`: Clear both left and right floats
///
/// # Clearance Calculation:
/// The element is pushed down so its top border edge is below the bottom edge
/// of all relevant floats.
pub fn compute_clearance(scoped: &mut ScopedDb, prev_floats: &[FloatBox]) -> Subpixels {
    let clear_value = scoped.query::<ClearQuery>();

    // Determine which floats to clear
    let floats_to_clear: Vec<&FloatBox> = match clear_value {
        CssValue::Keyword(CssKeyword::Left) => prev_floats
            .iter()
            .filter(|f| f.direction == FloatDirection::Left)
            .collect(),
        CssValue::Keyword(CssKeyword::Right) => prev_floats
            .iter()
            .filter(|f| f.direction == FloatDirection::Right)
            .collect(),
        CssValue::Keyword(CssKeyword::Both) => prev_floats.iter().collect(),
        CssValue::Keyword(CssKeyword::None) | _ => return 0,
    };

    if floats_to_clear.is_empty() {
        return 0;
    }

    // Find the bottom edge of the lowest float to clear
    floats_to_clear
        .iter()
        .map(|f| f.block_end())
        .max()
        .unwrap_or(0)
}

// ============================================================================
// Float Shrink-Wrapping
// ============================================================================

/// Compute the shrink-wrapped size for a floated element.
///
/// Floated elements establish a shrink-wrap width (fit-content) unless
/// an explicit width is specified.
///
/// The shrink-wrap width is:
/// - min(max(preferred minimum width, available width), preferred width)
/// - In practice: fit content to the narrowest reasonable width without overflow
pub fn compute_float_shrink_wrap_size(
    scoped: &mut ScopedDb,
    explicit_size: Option<Subpixels>,
) -> Subpixels {
    if let Some(size) = explicit_size {
        return size;
    }

    // Get the intrinsic size (preferred width based on content)
    let intrinsic_size = scoped.query::<SizeQuery<InlineMarker, crate::IntrinsicMarker>>();

    // Get the available width from parent
    let parent_size = scoped.parent::<SizeQuery<InlineMarker, ConstrainedMarker>>();

    // Return the minimum of intrinsic and available width
    intrinsic_size.min(parent_size)
}

// ============================================================================
// BFC Interaction
// ============================================================================

/// Compute available width for content alongside floats.
///
/// When an element flows alongside floats (doesn't clear them), its available
/// width is reduced by the width of the floats.
///
/// This is used for:
/// - Line boxes in inline formatting contexts
/// - Block boxes that don't establish a BFC
pub fn compute_available_width_with_floats(
    scoped: &mut ScopedDb,
    parent_width: Subpixels,
    block_offset: Subpixels,
) -> Subpixels {
    let prev_floats = collect_previous_floats(scoped);

    let mut left_intrusion = 0;
    let mut right_intrusion = 0;

    for float in prev_floats {
        // Check if this float overlaps vertically with the current position
        if block_offset >= float.block_offset && block_offset < float.block_end() {
            match float.direction {
                FloatDirection::Left => {
                    left_intrusion = left_intrusion.max(float.inline_size);
                }
                FloatDirection::Right => {
                    right_intrusion = right_intrusion.max(float.inline_size);
                }
            }
        }
    }

    parent_width - left_intrusion - right_intrusion
}

/// Check if an element should avoid floats.
///
/// Elements that establish a BFC do not overlap with floats. Their border box
/// is positioned to avoid floats, effectively creating a column alongside them.
pub fn should_avoid_floats(scoped: &mut ScopedDb) -> bool {
    super::bfc::establishes_bfc(scoped)
}

/// Compute the inline offset for an element that avoids floats.
///
/// If the element establishes a BFC, it is positioned to the right of left floats
/// (or to the left of right floats if RTL).
pub fn compute_float_avoiding_offset(
    scoped: &mut ScopedDb,
    default_offset: Subpixels,
    block_offset: Subpixels,
) -> Subpixels {
    if !should_avoid_floats(scoped) {
        return default_offset;
    }

    let prev_floats = collect_previous_floats(scoped);

    // Find the right edge of left floats at this vertical position
    let right_of_left_floats = prev_floats
        .iter()
        .filter(|f| {
            f.direction == FloatDirection::Left
                && block_offset >= f.block_offset
                && block_offset < f.block_end()
        })
        .map(|f| f.inline_end())
        .max()
        .unwrap_or(default_offset);

    right_of_left_floats.max(default_offset)
}

// ============================================================================
// Public API
// ============================================================================

/// Check if a node is floated.
pub fn is_float(scoped: &mut ScopedDb) -> bool {
    let float_value = scoped.query::<FloatQuery>();
    !matches!(float_value, CssValue::Keyword(CssKeyword::None))
}

/// Get the float direction for a floated element.
pub fn get_float_direction(scoped: &mut ScopedDb) -> Option<FloatDirection> {
    let float_value = scoped.query::<FloatQuery>();
    match float_value {
        CssValue::Keyword(CssKeyword::Left) => Some(FloatDirection::Left),
        CssValue::Keyword(CssKeyword::Right) => Some(FloatDirection::Right),
        _ => None,
    }
}
