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
//! - `PropertyResolver` trait provides CSS property access and tree navigation
//! - Formulas are non-generic — they use `NodeId` + `&dyn PropertyResolver`
//! - No separate dependency tracking — the formula tree is the dependency graph
//! - **Construct formulas using the macros only** — never build `Formula`
//!   variants directly in query code.

#[macro_use]
mod macros;
mod resolver;

pub use resolver::{ResolveContext, FONT_SIZE_FORMULA};

use lightningcss::properties::PropertyId;

use crate::{MultiRelationship, NodeId, SingleRelationship, Subpixel};

// ============================================================================
// PropertyResolver trait
// ============================================================================

/// Trait for resolving CSS properties and navigating the DOM tree.
///
/// This replaces the previous `StylerAccess` trait by separating node
/// identity from property resolution. Instead of boxing trait objects
/// for each node, callers pass `NodeId` values and the resolver looks
/// up properties directly from the database and tree.
///
/// Implemented in `rewrite_css` (as `CssPropertyResolver`) to avoid
/// circular dependencies — `rewrite_core` defines the trait but does
/// not depend on `rewrite_css` or `rewrite_html`.
pub trait PropertyResolver: Send + Sync {
    /// Query a CSS property for a node, converted to pixels.
    fn get_property(&self, node: NodeId, prop_id: &PropertyId<'static>) -> Option<Subpixel>;

    /// Query the raw CSS property for a node (for display-mode dispatch).
    fn get_css_property(
        &self,
        node: NodeId,
        prop_id: &PropertyId<'static>,
    ) -> Option<lightningcss::properties::Property<'static>>;

    /// Get the parent of a node, or `None` if it's the root.
    fn parent(&self, node: NodeId) -> Option<NodeId>;

    /// Get all direct children of a node (in reverse DOM order, as stored).
    fn children(&self, node: NodeId) -> Vec<NodeId>;

    /// Get previous siblings (closest first, DOM order).
    fn prev_siblings(&self, node: NodeId) -> Vec<NodeId>;

    /// Get next siblings (closest first, DOM order).
    fn next_siblings(&self, node: NodeId) -> Vec<NodeId>;

    /// Get the viewport width in pixels.
    fn viewport_width(&self) -> u32;

    /// Get the viewport height in pixels.
    fn viewport_height(&self) -> u32;

    /// Whether a node is intrinsic (text node, replaced element).
    fn is_intrinsic(&self, node: NodeId) -> bool;

    /// Whether a node is a DOM element (not text, comment, or document).
    fn is_element(&self, node: NodeId) -> bool;

    /// Get the text content of a node, if it is a text node.
    fn text_content(&self, node: NodeId) -> Option<String>;

    /// Measure text with explicitly provided font size.
    ///
    /// Font size is resolved by the caller through the formula cache,
    /// making the dependency on inherited font-size explicit.
    fn measure_text(
        &self,
        node: NodeId,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> Option<TextMeasurement>;
}

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

/// Query function type — takes a node ID and property resolver, returns a formula.
///
/// Returns `None` if the query cannot determine a formula (e.g., missing CSS
/// properties). Function pointers are `'static` so they can be stored in
/// static `Formula` values.
pub type QueryFn = fn(NodeId, &dyn PropertyResolver) -> Option<&'static Formula>;

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
    ctx: &dyn PropertyResolver,
    resolve: &mut dyn FnMut(&'static Formula, NodeId) -> Option<Subpixel>,
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

/// Dependency information for a formula - describes what relationships affect it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaDependency {
    /// Depends on the node's own CSS properties.
    SelfCss,
    /// Depends on the parent node.
    Parent,
    /// Depends on children.
    Children,
    /// Depends on siblings (prev, next, or all).
    Siblings,
    /// Depends on viewport dimensions.
    Viewport,
    /// No external dependencies (constant).
    None,
}

impl Formula {
    /// Collect all CSS property IDs that this formula reads.
    ///
    /// Walks the formula tree and collects all `CssValue` and `CssValueOrDefault`
    /// property IDs. Used to determine which cached values need re-evaluation
    /// when a CSS property changes.
    pub fn css_dependencies(&self, out: &mut Vec<PropertyId<'static>>) {
        match self {
            Formula::CssValue(prop_id) | Formula::CssValueOrDefault(prop_id, _) => {
                if !out.contains(prop_id) {
                    out.push(prop_id.clone());
                }
            }
            Formula::BinOp(_, lhs, rhs) => {
                lhs.css_dependencies(out);
                rhs.css_dependencies(out);
            }
            Formula::Related(_, _query_fn) => {
                // Query functions return formulas dynamically - we can't statically
                // determine their CSS dependencies without running them.
                // This is a limitation; for now we assume Related formulas might
                // depend on any property.
            }
            Formula::Aggregate(_, _list) => {
                // Similarly, aggregate query functions are dynamic.
            }
            Formula::LineAggregate(params) => {
                params.available_main.css_dependencies(out);
                params.gap.css_dependencies(out);
                params.line_gap.css_dependencies(out);
            }
            Formula::LineItemAggregate(params) => {
                params.available_main.css_dependencies(out);
                params.gap.css_dependencies(out);
            }
            Formula::PrevLinesAggregate(params) => {
                params.available_main.css_dependencies(out);
                params.gap.css_dependencies(out);
                params.line_gap.css_dependencies(out);
            }
            // These don't read CSS properties directly
            Formula::Constant(_)
            | Formula::ViewportWidth
            | Formula::ViewportHeight
            | Formula::InlineMeasure(_, _)
            | Formula::Imperative(_) => {}
        }
    }

    /// Check if this formula depends on a specific CSS property.
    pub fn depends_on_css_property(&self, prop_id: &PropertyId<'static>) -> bool {
        match self {
            Formula::CssValue(p) | Formula::CssValueOrDefault(p, _) => p == prop_id,
            Formula::BinOp(_, lhs, rhs) => {
                lhs.depends_on_css_property(prop_id) || rhs.depends_on_css_property(prop_id)
            }
            Formula::LineAggregate(params) => {
                params.available_main.depends_on_css_property(prop_id)
                    || params.gap.depends_on_css_property(prop_id)
                    || params.line_gap.depends_on_css_property(prop_id)
            }
            Formula::LineItemAggregate(params) => {
                params.available_main.depends_on_css_property(prop_id)
                    || params.gap.depends_on_css_property(prop_id)
            }
            Formula::PrevLinesAggregate(params) => {
                params.available_main.depends_on_css_property(prop_id)
                    || params.gap.depends_on_css_property(prop_id)
                    || params.line_gap.depends_on_css_property(prop_id)
            }
            // These don't read CSS properties directly, but Related/Aggregate
            // might through their query functions - we can't know statically
            Formula::Related(_, _) | Formula::Aggregate(_, _) | Formula::Imperative(_) => {
                // Conservative: assume they might depend on any property
                true
            }
            Formula::InlineMeasure(_, _) => matches!(
                prop_id,
                PropertyId::FontSize
                    | PropertyId::FontFamily
                    | PropertyId::FontWeight
                    | PropertyId::FontStyle
                    | PropertyId::LineHeight
                    | PropertyId::WhiteSpace
            ),
            Formula::Constant(_) | Formula::ViewportWidth | Formula::ViewportHeight => false,
        }
    }

    /// Collect all dependency types by walking the formula tree.
    ///
    /// Unlike `dependencies()` which returns a static slice (and must
    /// be conservative for `BinOp`), this walks both sides of binary
    /// operations and deduplicates.
    pub fn collect_dependencies(&self, out: &mut Vec<FormulaDependency>) {
        for &dep in self.dependencies() {
            if !out.contains(&dep) {
                out.push(dep);
            }
        }
        // Walk into BinOp children for precise dependencies.
        if let Formula::BinOp(_, lhs, rhs) = self {
            lhs.collect_dependencies(out);
            rhs.collect_dependencies(out);
        }
    }

    /// Returns what this formula depends on.
    ///
    /// This is used to determine which nodes need re-resolution when
    /// a node is added or a property changes.
    pub fn dependencies(&self) -> &'static [FormulaDependency] {
        match self {
            Formula::Constant(_) => &[FormulaDependency::None],
            Formula::ViewportWidth | Formula::ViewportHeight => &[FormulaDependency::Viewport],
            Formula::CssValue(_) | Formula::CssValueOrDefault(_, _) => {
                &[FormulaDependency::SelfCss]
            }
            Formula::Related(rel, _) => match rel {
                SingleRelationship::Parent | SingleRelationship::BlockContainer => {
                    &[FormulaDependency::Parent]
                }
                SingleRelationship::Self_ => &[FormulaDependency::SelfCss],
                SingleRelationship::PrevSibling => &[FormulaDependency::Siblings],
            },
            Formula::Aggregate(_, list) => {
                let FormulaList::Related(rel, _) = list;
                match rel {
                    MultiRelationship::Children | MultiRelationship::OrderedChildren => {
                        &[FormulaDependency::Children]
                    }
                    MultiRelationship::PrevSiblings
                    | MultiRelationship::NextSiblings
                    | MultiRelationship::OrderedPrevSiblings
                    | MultiRelationship::Siblings => &[FormulaDependency::Siblings],
                }
            }
            Formula::BinOp(_, lhs, rhs) => {
                // For binop, we'd need to combine dependencies - simplified for now
                let _ = (lhs, rhs);
                &[
                    FormulaDependency::SelfCss,
                    FormulaDependency::Parent,
                    FormulaDependency::Children,
                    FormulaDependency::Siblings,
                ]
            }
            Formula::InlineMeasure(_, _) => {
                &[
                    FormulaDependency::SelfCss,
                    FormulaDependency::Children,
                    FormulaDependency::Parent,
                ]
            }
            Formula::LineAggregate(_) => &[FormulaDependency::Children],
            Formula::LineItemAggregate(params) => match params.relationship {
                MultiRelationship::Children | MultiRelationship::OrderedChildren => {
                    &[FormulaDependency::Children, FormulaDependency::Parent]
                }
                _ => &[FormulaDependency::Siblings, FormulaDependency::Parent],
            },
            Formula::PrevLinesAggregate(_) => {
                &[FormulaDependency::Siblings, FormulaDependency::Parent]
            }
            Formula::Imperative(_) => {
                // Imperative functions can depend on anything
                &[
                    FormulaDependency::SelfCss,
                    FormulaDependency::Parent,
                    FormulaDependency::Children,
                    FormulaDependency::Siblings,
                ]
            }
        }
    }
}
