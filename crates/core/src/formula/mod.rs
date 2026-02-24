//! Formula computation graphs for layout values.
//!
//! This module provides:
//! - `Formula`: Computation graph for a single layout value
//! - `FormulaList`: Multi-value sources from related nodes
//! - `ResolveContext`: Memoized formula evaluation
//! - Construction macros: `constant!`, `css_val!`, `css_prop!`, `related!`,
//!   `aggregate!`, `add!`, `sub!`, `mul!`, `div!`, etc.
//!
//! Key design:
//! - Formulas are pure arithmetic over values from self/parent/children
//! - `StylerAccess` trait provides CSS property access and tree navigation
//! - Formulas are non-generic — they use `&dyn StylerAccess` for dispatch
//! - No separate dependency tracking — the formula tree is the dependency graph
//! - **Construct formulas using the macros only** — never build `Formula`
//!   variants directly in query code.

#[macro_use]
mod macros;
mod resolver;

pub use resolver::{ResolveContext, StylerAccess};

use lightningcss::properties::PropertyId;

use crate::{MultiRelationship, NodeId, SingleRelationship, Subpixel};

// ============================================================================
// Inline measurement parameters
// ============================================================================

/// Which axis to measure inline content along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureAxis {
    /// Measure width (horizontal extent).
    Width,
    /// Measure height (vertical extent).
    Height,
}

/// How to measure inline content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureMode {
    /// Wrap to containing block width (default for inline layout).
    FitAvailable,
    /// Minimum content size: the narrowest the content can be without
    /// overflow (e.g., longest word for text).
    MinContent,
    /// Maximum content size: the widest the content would be with no
    /// line breaks (single line for text).
    MaxContent,
    /// Measure the baseline offset (distance from top of content to
    /// first baseline).
    Baseline,
}

// ============================================================================
// Text measurement result
// ============================================================================

/// Result of measuring inline text content.
///
/// Returned by `StylerAccess::measure_text` so that the resolver can
/// extract width, height, or baseline as needed by the `MeasureMode`.
#[derive(Debug, Clone, Copy)]
pub struct TextMeasurement {
    /// Width of the measured text in pixels.
    pub width: f32,
    /// Height of the measured text in pixels.
    pub height: f32,
    /// Ascent above the baseline in pixels (positive upward).
    pub ascent: f32,
    /// Descent below the baseline in pixels (positive downward).
    pub descent: f32,
}

// ============================================================================
// Operations
// ============================================================================

/// Arithmetic operations for formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Add,
    Sub,
    Mul,
    Div,
    Max,
    Min,
}

// ============================================================================
// Query function type
// ============================================================================

/// Query function type - takes a styler trait object and returns a formula.
/// Returns None if the query lacks confidence (missing CSS properties).
pub type QueryFn = fn(&dyn StylerAccess) -> Option<&'static Formula>;

/// Batch-returning imperative resolver function.
///
/// Unlike declarative formulas, imperative functions can perform arbitrary
/// computation including iteration and conditionals on computed values.
/// They return results for multiple nodes at once; the resolver batch-inserts
/// all results into the cache so subsequent sibling lookups are O(1).
///
/// The `resolve` callback allows reading any other formula value through
/// the same cache context.
pub type ImperativeFn = fn(
    node: NodeId,
    styler: &dyn StylerAccess,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>>;

// ============================================================================
// Formula List
// ============================================================================

/// A source of multiple values from related nodes.
///
/// Construct via the `aggregate!` macro rather than building directly.
pub enum FormulaList {
    /// Run a query on each node in a multi-relationship.
    Related(MultiRelationship, QueryFn),
}

/// Aggregation operations that reduce a list to a single value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregation {
    Sum,
    Max,
    Min,
    Average,
    /// Count of nodes that return `Some` from the query function.
    Count,
}

// ============================================================================
// Formula - Computation graph
// ============================================================================

/// A formula describing how to compute a layout value.
///
/// Uses `dyn StylerAccess` for CSS property access and tree navigation,
/// so formulas can live in `static` variables without generic parameters.
///
/// **Construct via macros only**: `constant!`, `css_val!`, `css_prop!`,
/// `related!`, `related_val!`, `aggregate!`, `add!`, `sub!`, `mul!`, `div!`,
/// `viewport_width!`, `viewport_height!`, `line_aggregate!`, `line_item_aggregate!`.
pub enum Formula {
    /// A constant value.
    Constant(Subpixel),

    /// Read a CSS property value (needs unit conversion).
    CssValue(PropertyId<'static>),

    /// Read a CSS property value, returning a default if unset.
    CssValueOrDefault(PropertyId<'static>, Subpixel),

    /// Run a query on a related node to get a formula, then resolve it
    /// in that node's context.
    Related(SingleRelationship, QueryFn),

    /// Aggregate a list into a single value.
    Aggregate(Aggregation, &'static FormulaList),

    /// The viewport width in pixels.
    ViewportWidth,

    /// The viewport height in pixels.
    ViewportHeight,

    /// Binary operation: lhs op rhs.
    BinOp(Operation, &'static Self, &'static Self),

    /// Inline content measurement — parameterized by axis and mode.
    ///
    /// Replaces the old `InlineWidth` / `InlineHeight` variants with a
    /// unified measurement that also supports min-content, max-content,
    /// and baseline queries.
    ///
    /// - `FitAvailable` + `Width`:  wrap to containing block, return width
    /// - `FitAvailable` + `Height`: wrap to containing block, return height
    /// - `MinContent`   + `Width`:  narrowest width without overflow
    /// - `MaxContent`   + `Width`:  widest width (no line breaks)
    /// - `Baseline`     + `Height`: distance from top to first baseline
    InlineMeasure(MeasureAxis, MeasureMode),

    /// Line-breaking aggregate over children.
    ///
    /// Groups children into lines by accumulating each child's main-axis
    /// size (from `item_main_size` query). When the accumulated size plus
    /// gap exceeds `available_main`, a new line starts. Then aggregates:
    /// - Within each line: combines item values using `within_line_agg`
    /// - Across lines: combines line values using `line_agg`
    ///
    /// Used for both inline IFC (line boxes) and flex-wrap (flex lines).
    LineAggregate(LineAggregateParams),

    /// Line-aware aggregate over siblings on the same line.
    ///
    /// Like `Aggregate`, but only considers siblings that share the same
    /// line as the current item. Line assignments are computed by walking
    /// the parent's children and breaking lines using `item_main_size`,
    /// `available_main`, and `gap` — identical to `LineAggregate`.
    ///
    /// The `relationship` field determines which siblings to include:
    /// - `PrevSiblings`: only same-line siblings before this item
    /// - `NextSiblings`: only same-line siblings after this item
    /// - `Children`: all items on the same line (used for totals like sum of grow)
    LineItemAggregate(LineItemAggregateParams),

    /// Aggregate over all lines before the current item's line.
    ///
    /// Computes line assignments (same as `LineAggregate`), finds which
    /// line the current item belongs to, then for each *previous* line:
    /// aggregates item values within the line using `within_line_agg`,
    /// then aggregates across those previous lines using `line_agg`.
    /// Also adds `line_gap * prev_line_count` to account for cross gaps.
    ///
    /// Used for cross-axis offsets in flex-wrap: the cross offset of an
    /// item equals the sum of all previous lines' max cross sizes plus
    /// the cross gaps between them.
    PrevLinesAggregate(PrevLinesAggregateParams),

    /// Imperative resolution with batch caching.
    ///
    /// Delegates to a Rust function that can perform arbitrary computation
    /// (iteration, conditionals on computed values, multi-step algorithms
    /// like CSS Flexbox §9.7 freeze-and-redistribute).
    ///
    /// The function returns results for multiple nodes at once. The resolver
    /// inserts all of them into the cache, so sibling lookups are O(1).
    Imperative(ImperativeFn),
}

/// Parameters for `Formula::LineAggregate`.
#[derive(Clone, Copy)]
pub struct LineAggregateParams {
    /// How to combine line values across lines (e.g., Sum for total cross
    /// height, Max for max main width).
    pub line_agg: Aggregation,
    /// How to combine item values within each line (e.g., Sum for main
    /// sizes, Max for cross sizes).
    pub within_line_agg: Aggregation,
    /// Query to get each child's main-axis size for line-breaking decisions.
    pub item_main_size: QueryFn,
    /// Query to get the value to aggregate per item.
    pub item_value: QueryFn,
    /// Formula for available main-axis space (resolved on the parent).
    pub available_main: &'static Formula,
    /// Formula for gap between items on a line (resolved on the parent).
    pub gap: &'static Formula,
    /// Formula for gap between lines (resolved on the parent).
    pub line_gap: &'static Formula,
}

/// Parameters for `Formula::LineItemAggregate`.
#[derive(Clone, Copy)]
pub struct LineItemAggregateParams {
    /// How to aggregate the values.
    pub agg: Aggregation,
    /// Which relationship to filter to same-line items.
    pub relationship: MultiRelationship,
    /// Query to get each sibling's value.
    pub query: QueryFn,
    /// Query to get each child's main-axis size (for line-breaking).
    pub item_main_size: QueryFn,
    /// Formula for available main-axis space (resolved on parent).
    pub available_main: &'static Formula,
    /// Formula for gap between items on a line (resolved on parent).
    pub gap: &'static Formula,
}

/// Parameters for `Formula::PrevLinesAggregate`.
#[derive(Clone, Copy)]
pub struct PrevLinesAggregateParams {
    /// How to combine line values across previous lines (e.g., Sum).
    pub line_agg: Aggregation,
    /// How to combine item values within each previous line (e.g., Max).
    pub within_line_agg: Aggregation,
    /// Query to get each child's main-axis size (for line-breaking).
    pub item_main_size: QueryFn,
    /// Query to get the value to aggregate per item.
    pub item_value: QueryFn,
    /// Formula for available main-axis space (resolved on the parent).
    pub available_main: &'static Formula,
    /// Formula for gap between items on a line (resolved on the parent).
    pub gap: &'static Formula,
    /// Formula for gap between lines (resolved on the parent).
    pub line_gap: &'static Formula,
}
