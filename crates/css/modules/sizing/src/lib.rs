//! CSS Sizing Module - Universal sizing system for all layout contexts
//!
//! [Spec: CSS Box Sizing Module Level 3](https://www.w3.org/TR/css-sizing-3/)
//! [Spec: CSS Sizing Level 3](https://www.w3.org/TR/css-sizing-3/)
//!
//! ## Architecture
//!
//! This module provides a **universal sizing entry point** that works for ALL elements
//! in ANY layout context (block, inline, flex, absolute, etc.). This eliminates:
//!
//! - Parallel sizing implementations across formatting contexts
//! - Inconsistent constraint application
//! - Bypassing the pipeline (the root cause of sizing bugs)
//!
//! ## Design
//!
//! ```text
//! Universal Entry Point: compute_element_size()
//!   ↓
//! 1. Resolve content-box size (specified, intrinsic, or auto)
//! 2. Transform to border-box (apply box-sizing)
//! 3. Apply min/max constraints
//!   ↓
//! Return final border-box size (f32 pixels)
//! ```
//!
//! ## Important: Separation of Concerns
//!
//! This module contains ONLY CSS Sizing spec code. It does NOT contain:
//! - Form control dimensions (HTML5 Rendering spec - belongs in `css_core` or html)
//! - Intrinsic sizing for replaced elements (belongs in `css_core`)
//! - DOM/layout tree access
//!
//! ## Spec Coverage Status
//!
//! - [Production] Universal sizing entry point (all formatting contexts)
//! - [Production] Box-sizing transformation (content-box, border-box)
//! - [Production] Min/max constraints
//! - [Production] Intrinsic sizing (min-content, max-content)
//! - [TODO] Aspect ratio preservation

use css_orchestrator::style_model::BoxSizing;

//=============================================================================
// Universal Sizing Context
//=============================================================================

/// Dimension being sized (width or height).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Width,
    Height,
}

/// Source of content-box size for an element.
///
/// This represents the various ways an element's content-box size can be determined
/// per CSS 2.2 §10.3 (width) and §10.6 (height).
#[derive(Debug, Clone, Copy)]
pub enum ContentSource {
    /// Explicitly specified size (e.g., `width: 100px`)
    Specified(f32),

    /// Intrinsic size from replaced element or form control
    /// (e.g., image dimensions, button intrinsic size)
    Intrinsic(f32),

    /// Percentage of containing block
    /// (e.g., `width: 50%` → 50% of parent's content width)
    Percentage { percent: f32, basis: f32 },

    /// Auto - depends on context (shrink-to-fit, fill-available, content-based, etc.)
    Auto(f32),
}

impl ContentSource {
    /// Resolve to a content-box size in pixels.
    #[inline]
    pub fn resolve(self) -> f32 {
        match self {
            Self::Percentage { percent, basis } => basis * percent,
            Self::Specified(pixels) | Self::Intrinsic(pixels) | Self::Auto(pixels) => pixels,
        }
    }
}

/// Context for computing element size.
///
/// This struct contains all information needed to compute an element's size
/// in any layout context (block, inline, flex, absolute, etc.).
#[derive(Debug, Clone, Copy)]
pub struct SizingContext {
    /// The content-box size source
    pub content: ContentSource,

    /// Box-sizing mode (content-box or border-box)
    pub box_sizing: BoxSizing,

    /// Sum of padding + border in this dimension (px)
    pub padding_border: f32,

    /// Minimum size constraint (border-box, px)
    pub min: Option<f32>,

    /// Maximum size constraint (border-box, px)
    pub max: Option<f32>,
}

//=============================================================================
// Phase 1: Transformation - Apply box-sizing rules
//=============================================================================

/// Transform content-box size to border-box based on box-sizing property.
///
/// [Spec: CSS Box Sizing Level 3 §3 Box Sizing]
///
/// ## Algorithm
///
/// ```text
/// box-sizing: content-box  → border_box = content + padding + border
/// box-sizing: border-box   → border_box = content (already includes padding/border)
/// ```
///
/// ## Important
///
/// This is the SINGLE SOURCE OF TRUTH for box-sizing transformation.
/// All height and width computations MUST use this function.
///
/// ## Examples
///
/// ```
/// # use css_sizing::apply_box_sizing;
/// # use css_orchestrator::style_model::BoxSizing;
/// let content = 100.0;
/// let padding_border = 20.0;
///
/// // content-box: need to add padding/border
/// assert_eq!(
///     apply_box_sizing(content, BoxSizing::ContentBox, padding_border),
///     120.0
/// );
///
/// // border-box: content value already includes padding/border
/// assert_eq!(
///     apply_box_sizing(content, BoxSizing::BorderBox, padding_border),
///     100.0
/// );
/// ```
#[inline]
pub fn apply_box_sizing(content_size: f32, box_sizing: BoxSizing, padding_border: f32) -> f32 {
    match box_sizing {
        BoxSizing::ContentBox => content_size + padding_border,
        BoxSizing::BorderBox => content_size,
    }
}

/// Apply min/max constraints to a border-box size.
///
/// [Spec: CSS Sizing Level 3 §4.5 Min/Max Constraints]
///
/// ## Algorithm
///
/// Per spec: "The min-width and max-width properties are applied after
/// the used value of width is computed."
///
/// ```text
/// 1. clamped = max(size, min_size)
/// 2. clamped = min(clamped, max_size)
/// ```
#[inline]
pub fn apply_constraints(size: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut result = size;

    if let Some(min_size) = min {
        result = result.max(min_size);
    }

    if let Some(max_size) = max {
        result = result.min(max_size);
    }

    result
}

//=============================================================================
// Universal Sizing Entry Point
//=============================================================================

/// **UNIVERSAL SIZING ENTRY POINT** - Compute element size for ANY layout context.
///
/// This is the SINGLE function that ALL formatting contexts (block, inline, flex, absolute, etc.)
/// MUST use to compute element sizes. This ensures:
///
/// - Consistent box-sizing transformation across all contexts
/// - Consistent constraint application
/// - No bypassing the pipeline (prevents sizing bugs)
///
/// [Spec: CSS 2.2 §10.3 Width Computation, §10.6 Height Computation]
///
/// ## Algorithm
///
/// ```text
/// 1. Resolve content-box size from source (specified/intrinsic/percentage/auto)
/// 2. Transform to border-box based on box-sizing property
/// 3. Apply min/max constraints to border-box size
/// 4. Return final border-box size
/// ```
///
/// ## Parameters
///
/// - `ctx`: Sizing context containing all necessary information
///
/// ## Returns
///
/// Final border-box size in pixels (f32).
///
/// ## Examples
///
/// ```
/// # use css_sizing::{SizingContext, ContentSource, Dimension, compute_element_size};
/// # use css_orchestrator::style_model::BoxSizing;
///
/// // Block element: width: 100px; padding: 10px; border: 5px; box-sizing: content-box
/// let ctx = SizingContext {
///     content: ContentSource::Specified(100.0),
///     box_sizing: BoxSizing::ContentBox,
///     padding_border: 30.0, // (10+10) padding + (5+5) border
///     min: None,
///     max: None,
/// };
/// assert_eq!(compute_element_size(ctx), 130.0); // 100 + 30
///
/// // Text input: intrinsic height 17px; padding: 8px; border: 2px; box-sizing: content-box
/// let ctx = SizingContext {
///     content: ContentSource::Intrinsic(17.0),
///     box_sizing: BoxSizing::ContentBox,
///     padding_border: 20.0, // (8+8) padding + (2+2) border
///     min: None,
///     max: None,
/// };
/// assert_eq!(compute_element_size(ctx), 37.0); // 17 + 20 (FIXES THE BUG!)
/// ```
#[inline]
pub fn compute_element_size(ctx: SizingContext) -> f32 {
    // Phase 1: Resolve content-box size
    let content_size = ctx.content.resolve();

    // Phase 2: Transform to border-box
    let border_box_size = apply_box_sizing(content_size, ctx.box_sizing, ctx.padding_border);

    // Phase 3: Apply constraints
    apply_constraints(border_box_size, ctx.min, ctx.max)
}

//=============================================================================
// Intrinsic Sizing
//=============================================================================

/// Intrinsic sizes of an element.
///
/// [Spec: CSS Sizing Level 3 §4 Intrinsic Size Determination]
/// <https://www.w3.org/TR/css-sizing-3/#intrinsic-sizes>
///
/// ## Definition
///
/// Intrinsic sizes are determined by the element's content, not by its containing block.
/// These are used by layout algorithms like Grid and Flexbox to size tracks/items.
///
/// - **Min-content inline**: Width when the element wraps as much as possible (zero-width container)
/// - **Max-content inline**: Width when the element doesn't wrap at all (infinite-width container)
/// - **Min-content block**: Height when laid out at min-content inline width
/// - **Max-content block**: Height when laid out at max-content inline width
///
/// ## Examples
///
/// For a text block with "Hello World":
/// - Min-content inline: width of "Hello" or "World" (whichever is wider, fully wrapped)
/// - Max-content inline: width of "Hello World" (no wrapping)
/// - Min-content block: height when text is at min-content width (tall, wrapped)
/// - Max-content block: height when text is at max-content width (short, unwrapped)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntrinsicSizes {
    /// Min-content inline size (width in horizontal writing mode).
    ///
    /// The smallest inline size the element can be without overflow, allowing maximum wrapping.
    /// For text, this is typically the width of the longest word.
    pub min_content_inline: f32,

    /// Max-content inline size (width in horizontal writing mode).
    ///
    /// The ideal inline size the element would take if given infinite available space.
    /// For text, this is the width with no wrapping.
    pub max_content_inline: f32,

    /// Min-content block size (height in horizontal writing mode).
    ///
    /// The block size when the element is laid out at min-content inline size.
    /// For text, this is the height when maximally wrapped.
    pub min_content_block: f32,

    /// Max-content block size (height in horizontal writing mode).
    ///
    /// The block size when the element is laid out at max-content inline size.
    /// For text, this is the height when not wrapped.
    pub max_content_block: f32,
}

impl IntrinsicSizes {
    /// Create a new `IntrinsicSizes` with all dimensions set to zero.
    ///
    /// Useful for elements with no content or replaced elements where intrinsic
    /// sizes are determined externally.
    pub fn zero() -> Self {
        Self {
            min_content_inline: 0.0,
            max_content_inline: 0.0,
            min_content_block: 0.0,
            max_content_block: 0.0,
        }
    }

    /// Create `IntrinsicSizes` for a fixed-size element.
    ///
    /// For replaced elements (images, form controls) where the intrinsic size
    /// is the same regardless of available space.
    pub fn fixed(inline: f32, block: f32) -> Self {
        Self {
            min_content_inline: inline,
            max_content_inline: inline,
            min_content_block: block,
            max_content_block: block,
        }
    }
}

impl Default for IntrinsicSizes {
    fn default() -> Self {
        Self::zero()
    }
}

//=============================================================================
// Legacy API (for backwards compatibility during migration)
//=============================================================================

/// Complete sizing pipeline: transform → constrain.
///
/// **DEPRECATED**: Use `compute_element_size()` with `SizingContext` instead.
///
/// This function is kept temporarily for backwards compatibility during migration.
/// All new code should use the universal sizing entry point.
///
/// [Spec: CSS 2.2 §10.3 Width Computation, §10.6 Height Computation]
///
/// ## Parameters
///
/// - `content_size`: The content-box size in pixels
/// - `box_sizing`: Box sizing mode (content-box or border-box)
/// - `padding_border`: Sum of padding + border in the sizing dimension
/// - `min`: Optional minimum size constraint (already in border-box)
/// - `max`: Optional maximum size constraint (already in border-box)
///
/// ## Returns
///
/// Final border-box size in pixels.
#[inline]
pub fn compute_used_size(
    content_size: f32,
    box_sizing: BoxSizing,
    padding_border: f32,
    min: Option<f32>,
    max: Option<f32>,
) -> f32 {
    // Delegate to universal entry point
    compute_element_size(SizingContext {
        content: ContentSource::Auto(content_size),
        box_sizing,
        padding_border,
        min,
        max,
    })
}
