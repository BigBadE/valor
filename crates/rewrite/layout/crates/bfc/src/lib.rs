/// Block Formatting Context (BFC) module implementing CSS 2.2 BFC rules.
///
/// This module handles:
/// - BFC establishment detection
/// - BFC isolation (prevents margin collapsing)
/// - Float containment (when implemented)
/// - Clearance calculation (when floats are implemented)
///
/// Spec: https://www.w3.org/TR/CSS22/visuren.html#block-formatting
use rewrite_core::ScopedDb;
use rewrite_css::{
    CssKeyword, CssValue, DisplayQuery, FloatQuery, OverflowQuery, PositionQuery, Subpixels,
};

/// Check if an element establishes a block formatting context (BFC).
///
/// # What is a BFC?
///
/// A block formatting context is a region of the page where block boxes are laid out.
/// It is an isolated environment where:
/// - Margins of adjacent boxes don't collapse across the BFC boundary
/// - The BFC contains internal floats (floats don't escape)
/// - The BFC doesn't overlap with external floats
///
/// # Elements that Establish a BFC:
///
/// 1. **Root element** (html)
/// 2. **Floats** (float: left or right)
/// 3. **Absolutely positioned** (position: absolute or fixed)
/// 4. **Inline-blocks** (display: inline-block)
/// 5. **Table cells** (display: table-cell)
/// 6. **Table captions** (display: table-caption)
/// 7. **Overflow not visible** (overflow: hidden, auto, scroll, clip)
/// 8. **Display: flow-root** (explicitly creates BFC)
/// 9. **Flex/Grid items** (direct children of flex/grid containers)
/// 10. **Contain: layout, content, or strict** (CSS Containment)
/// 11. **Column containers** (column-count or column-width)
///
/// # Effects of Establishing a BFC:
///
/// ## Margin Collapsing
/// Margins do not collapse:
/// - Between a BFC root and its descendants
/// - Between elements in different BFCs
///
/// Example:
/// ```html
/// <div style="overflow: hidden"> <!-- Establishes BFC -->
///   <div style="margin-top: 20px">Child</div>
/// </div>
/// ```
/// The parent's margin and child's margin do NOT collapse because the parent
/// establishes a BFC.
///
/// ## Float Containment
/// A BFC contains floats within it. Floats inside a BFC do not escape.
///
/// Example:
/// ```html
/// <div style="overflow: hidden"> <!-- BFC contains float -->
///   <div style="float: left">Float</div>
/// </div>
/// ```
/// The parent's height includes the float.
///
/// ## Float Avoidance
/// A BFC does not overlap with floats outside of it.
///
/// # Implementation Notes:
///
/// This function checks all conditions that establish a BFC according to
/// CSS 2.2 and CSS Display Level 3.
pub fn establishes_bfc(scoped: &mut ScopedDb) -> bool {
    let node = scoped.node();

    // 1. Check if element is floated
    // Note: float defaults to 'none' per CSS spec
    let float = scoped.query::<FloatQuery>();
    let is_floated = match float {
        CssValue::Keyword(CssKeyword::None) => false,
        CssValue::Keyword(CssKeyword::Initial) => false, // Initial defaults to none
        CssValue::Keyword(CssKeyword::Inherit) => false, // Inherit from parent (TODO: check parent)
        _ => true, // left, right, inline-start, inline-end all establish BFC
    };

    if is_floated {
        eprintln!(
            "establishes_bfc: node={:?} is floated, float={:?}",
            node, float
        );
        return true;
    }

    // 2. Check if absolutely positioned
    // Note: position defaults to 'static' per CSS spec
    let position = scoped.query::<PositionQuery>();
    let is_absolutely_positioned = match position {
        CssValue::Keyword(CssKeyword::Absolute) | CssValue::Keyword(CssKeyword::Fixed) => true,
        _ => false, // static, relative, sticky, initial, inherit are not absolutely positioned
    };

    if is_absolutely_positioned {
        eprintln!(
            "establishes_bfc: node={:?} is absolutely positioned, position={:?}",
            node, position
        );
        return true;
    }

    // 3. Check display value
    let display = scoped.query::<DisplayQuery>();
    match display {
        // Inline-block establishes BFC
        CssValue::Keyword(CssKeyword::InlineBlock) => {
            eprintln!("establishes_bfc: node={:?} is inline-block", node);
            return true;
        }

        // Table cells establish BFC
        CssValue::Keyword(CssKeyword::TableCell) => {
            eprintln!("establishes_bfc: node={:?} is table-cell", node);
            return true;
        }

        // Flow-root explicitly creates BFC (CSS Display Level 3)
        CssValue::Keyword(CssKeyword::FlowRoot) => {
            eprintln!("establishes_bfc: node={:?} is flow-root", node);
            return true;
        }

        // Flex and Grid containers establish BFC for their contents
        CssValue::Keyword(CssKeyword::Flex)
        | CssValue::Keyword(CssKeyword::InlineFlex)
        | CssValue::Keyword(CssKeyword::Grid)
        | CssValue::Keyword(CssKeyword::InlineGrid) => {
            eprintln!("establishes_bfc: node={:?} is flex/grid container", node);
            return true;
        }

        _ => {}
    }

    // 4. Check overflow (anything except visible/initial establishes BFC)
    // Note: Initial/unset defaults to visible per CSS spec
    let overflow = scoped.query::<OverflowQuery>();
    let establishes_from_overflow = match overflow {
        CssValue::Keyword(CssKeyword::Visible) => false,
        CssValue::Keyword(CssKeyword::Initial) => false, // Initial defaults to visible
        CssValue::Keyword(CssKeyword::Inherit) => false, // Inherit from parent (TODO: check parent value)
        _ => true, // hidden, auto, scroll, clip, etc. all establish BFC
    };

    if establishes_from_overflow {
        eprintln!(
            "establishes_bfc: node={:?} has overflow={:?}",
            node, overflow
        );
        return true;
    }

    // 5. Check if parent is flex or grid (children establish BFC)
    if is_flex_or_grid_item(scoped) {
        eprintln!("establishes_bfc: node={:?} is flex/grid item", node);
        return true;
    }

    // 6. TODO: Check contain property (layout, content, strict)
    // 7. TODO: Check column-count or column-width

    // Does not establish BFC
    false
}

/// Check if the current element is a flex or grid item.
///
/// Flex and grid items establish a BFC for their contents, even if they
/// have display: block.
fn is_flex_or_grid_item(scoped: &mut ScopedDb) -> bool {
    let Some(parent) = scoped.parent_id() else {
        return false;
    };

    let parent_display = scoped.node_query::<DisplayQuery>(parent);
    matches!(
        parent_display,
        CssValue::Keyword(CssKeyword::Flex)
            | CssValue::Keyword(CssKeyword::InlineFlex)
            | CssValue::Keyword(CssKeyword::Grid)
            | CssValue::Keyword(CssKeyword::InlineGrid)
    )
}

/// Check if an element creates an independent formatting context.
///
/// This is a broader concept than BFC. Elements that create an independent
/// formatting context include:
/// - BFC roots
/// - Inline formatting context roots (inline-block, etc.)
/// - Flex formatting context roots
/// - Grid formatting context roots
/// - Table formatting context roots
///
/// This is useful for determining containment boundaries.
pub fn creates_formatting_context(scoped: &mut ScopedDb) -> bool {
    let display = scoped.query::<DisplayQuery>();

    match display {
        // Block-level formatting contexts
        CssValue::Keyword(CssKeyword::FlowRoot) => true,

        // Inline-level formatting contexts
        CssValue::Keyword(CssKeyword::InlineBlock) => true,

        // Flex formatting contexts
        CssValue::Keyword(CssKeyword::Flex) | CssValue::Keyword(CssKeyword::InlineFlex) => true,

        // Grid formatting contexts
        CssValue::Keyword(CssKeyword::Grid) | CssValue::Keyword(CssKeyword::InlineGrid) => true,

        // Table formatting contexts
        CssValue::Keyword(CssKeyword::Table) | CssValue::Keyword(CssKeyword::TableCell) => true,

        // Block may establish BFC based on other properties
        CssValue::Keyword(CssKeyword::Block) => establishes_bfc(scoped),

        _ => false,
    }
}

/// Find the nearest ancestor that establishes a BFC.
///
/// This is used for:
/// - Determining the containing block for floats
/// - Finding the BFC root for margin collapsing
/// - Determining float interaction boundaries
///
/// Returns the NodeId of the BFC root, or None if no BFC ancestor exists
/// (which would mean the root element).
pub fn find_bfc_root(scoped: &mut ScopedDb) -> Option<rewrite_core::NodeId> {
    let mut current = scoped.parent_id();

    while let Some(node) = current {
        // Check if this node establishes a BFC
        let establishes = {
            // We need to query this node's properties
            let float = scoped.node_query::<FloatQuery>(node);
            if !matches!(float, CssValue::Keyword(CssKeyword::None)) {
                true
            } else {
                let position = scoped.node_query::<PositionQuery>(node);
                if matches!(
                    position,
                    CssValue::Keyword(CssKeyword::Absolute) | CssValue::Keyword(CssKeyword::Fixed)
                ) {
                    true
                } else {
                    let display = scoped.node_query::<DisplayQuery>(node);
                    let overflow = scoped.node_query::<OverflowQuery>(node);
                    matches!(
                        display,
                        CssValue::Keyword(CssKeyword::InlineBlock)
                            | CssValue::Keyword(CssKeyword::TableCell)
                            | CssValue::Keyword(CssKeyword::FlowRoot)
                            | CssValue::Keyword(CssKeyword::Flex)
                            | CssValue::Keyword(CssKeyword::InlineFlex)
                            | CssValue::Keyword(CssKeyword::Grid)
                            | CssValue::Keyword(CssKeyword::InlineGrid)
                    ) || !matches!(overflow, CssValue::Keyword(CssKeyword::Visible))
                }
            }
        };

        if establishes {
            return Some(node);
        }

        // Move to parent
        current = scoped.node_parent(node);
    }

    None
}

/// Check if margins can collapse across the boundary of this element.
///
/// Margins cannot collapse if:
/// - The element establishes a BFC
/// - The element is absolutely positioned
/// - The element is floated
///
/// This is a convenience function used by margin collapsing logic.
pub fn blocks_margin_collapsing(scoped: &mut ScopedDb) -> bool {
    establishes_bfc(scoped)
}

// Make establishes_bfc available within the crate for use by other modules

// ============================================================================
// Float Interaction (Placeholder for Future Implementation)
// ============================================================================

/// Check if an element should avoid floats.
///
/// Elements that establish a BFC do not overlap with floats outside of them.
/// This affects their positioning and sizing.
///
/// TODO: Implement when float support is added.
#[allow(dead_code)]
pub fn avoids_floats(scoped: &mut ScopedDb) -> bool {
    establishes_bfc(scoped)
}

/// Check if an element contains floats.
///
/// Elements that establish a BFC contain floats within them. This means
/// the element's height should include its floated descendants.
///
/// TODO: Implement when float support is added.
#[allow(dead_code)]
pub fn contains_floats(scoped: &mut ScopedDb) -> bool {
    establishes_bfc(scoped)
}

// ============================================================================
// Clearance (Placeholder for Future Implementation)
// ============================================================================

/// Compute clearance for an element with the 'clear' property.
///
/// Clearance is additional space added above an element to ensure it
/// appears below floated elements.
///
/// The clear property values:
/// - none: no clearance
/// - left: clear left floats
/// - right: clear right floats
/// - both: clear both left and right floats
///
/// TODO: Implement when float support is added.
#[allow(dead_code)]
pub fn compute_clearance(_scoped: &mut ScopedDb) -> Subpixels {
    // Placeholder: return 0 until floats are implemented
    0
}
