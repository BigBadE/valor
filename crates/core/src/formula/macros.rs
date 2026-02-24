//! Macros for constructing static `Formula` trees.
//!
//! Every macro returns `&'static Formula`. Macros compose freely:
//! ```ignore
//! sub!(
//!     related!(Self_, size_query, axis),
//!     css_prop!(PaddingLeft),
//!     css_prop!(PaddingRight),
//! )
//! ```

// ============================================================================
// Leaf values
// ============================================================================

/// Constant formula: `Formula::Constant(value)`.
#[macro_export]
macro_rules! constant {
    ($value:expr) => {{
        static F: $crate::Formula = $crate::Formula::Constant($value);
        &F
    }};
}

/// Read a CSS property (required — returns `None` if unset):
/// `Formula::CssValue(PropertyId)`.
#[macro_export]
macro_rules! css_val {
    ($prop:ident) => {{
        static F: $crate::Formula =
            $crate::Formula::CssValue(::lightningcss::properties::PropertyId::$prop);
        &F
    }};
}

/// Read a CSS property with zero default:
/// `Formula::CssValueOrDefault(PropertyId, Subpixel::ZERO)`.
#[macro_export]
macro_rules! css_prop {
    ($prop:ident) => {{
        static F: $crate::Formula = $crate::Formula::CssValueOrDefault(
            ::lightningcss::properties::PropertyId::$prop,
            $crate::Subpixel::ZERO,
        );
        &F
    }};
}

/// Viewport width: `Formula::ViewportWidth`.
#[macro_export]
macro_rules! viewport_width {
    () => {{
        static F: $crate::Formula = $crate::Formula::ViewportWidth;
        &F
    }};
}

/// Viewport height: `Formula::ViewportHeight`.
#[macro_export]
macro_rules! viewport_height {
    () => {{
        static F: $crate::Formula = $crate::Formula::ViewportHeight;
        &F
    }};
}

/// Inline content width (fit-available mode).
///
/// Wraps text to the containing block width and returns the width.
#[macro_export]
macro_rules! inline_width {
    () => {{
        static F: $crate::Formula = $crate::Formula::InlineMeasure(
            $crate::MeasureAxis::Width,
            $crate::MeasureMode::FitAvailable,
        );
        &F
    }};
}

/// Inline content height (fit-available mode).
///
/// Wraps text to the containing block width and returns the height.
#[macro_export]
macro_rules! inline_height {
    () => {{
        static F: $crate::Formula = $crate::Formula::InlineMeasure(
            $crate::MeasureAxis::Height,
            $crate::MeasureMode::FitAvailable,
        );
        &F
    }};
}

/// Minimum content width.
///
/// The narrowest the content can be without overflow (e.g., longest word).
#[macro_export]
macro_rules! min_content_width {
    () => {{
        static F: $crate::Formula = $crate::Formula::InlineMeasure(
            $crate::MeasureAxis::Width,
            $crate::MeasureMode::MinContent,
        );
        &F
    }};
}

/// Minimum content height.
///
/// The minimum height when content is at its narrowest.
#[macro_export]
macro_rules! min_content_height {
    () => {{
        static F: $crate::Formula = $crate::Formula::InlineMeasure(
            $crate::MeasureAxis::Height,
            $crate::MeasureMode::MinContent,
        );
        &F
    }};
}

/// Maximum content width.
///
/// The widest the content would be with no line breaks.
#[macro_export]
macro_rules! max_content_width {
    () => {{
        static F: $crate::Formula = $crate::Formula::InlineMeasure(
            $crate::MeasureAxis::Width,
            $crate::MeasureMode::MaxContent,
        );
        &F
    }};
}

/// Inline baseline offset.
///
/// Distance from the top of the content to the first baseline.
#[macro_export]
macro_rules! inline_baseline {
    () => {{
        static F: $crate::Formula = $crate::Formula::InlineMeasure(
            $crate::MeasureAxis::Height,
            $crate::MeasureMode::Baseline,
        );
        &F
    }};
}

// ============================================================================
// Relationships
// ============================================================================

/// Query a related node: `Formula::Related(rel, query_fn)`.
///
/// Two forms:
/// - `related!(Rel, query_fn)` — direct function pointer
/// - `related!(Rel, base_fn, axis)` — dispatches `base_fn(styler, axis)` via axis match
#[macro_export]
macro_rules! related {
    ($rel:ident, $query:expr) => {{
        static F: $crate::Formula =
            $crate::Formula::Related($crate::SingleRelationship::$rel, $query);
        &F
    }};
    ($rel:ident, $base:path, $axis:expr) => {
        match $axis {
            $crate::Axis::Horizontal => {
                static F: $crate::Formula =
                    $crate::Formula::Related($crate::SingleRelationship::$rel, {
                        fn wrap(
                            sty: &dyn $crate::StylerAccess,
                        ) -> ::core::option::Option<&'static $crate::Formula> {
                            $base(sty, $crate::Axis::Horizontal)
                        }
                        wrap as $crate::QueryFn
                    });
                &F
            }
            $crate::Axis::Vertical => {
                static F: $crate::Formula =
                    $crate::Formula::Related($crate::SingleRelationship::$rel, {
                        fn wrap(
                            sty: &dyn $crate::StylerAccess,
                        ) -> ::core::option::Option<&'static $crate::Formula> {
                            $base(sty, $crate::Axis::Vertical)
                        }
                        wrap as $crate::QueryFn
                    });
                &F
            }
        }
    };
}

/// Query a related node with a formula expression:
/// `Formula::Related(rel, |_| Some(formula))`.
///
/// Unlike `related!`, this takes a formula expression instead of a `QueryFn`.
/// The formula is evaluated in the related node's context.
#[macro_export]
macro_rules! related_val {
    ($rel:ident, $formula:expr) => {{
        static F: $crate::Formula = $crate::Formula::Related($crate::SingleRelationship::$rel, {
            fn wrap(
                _sty: &dyn $crate::StylerAccess,
            ) -> ::core::option::Option<&'static $crate::Formula> {
                ::core::option::Option::Some($formula)
            }
            wrap as $crate::QueryFn
        });
        &F
    }};
}

/// Aggregate over related nodes:
/// `Formula::Aggregate(agg, &FormulaList::Related(rel, query_fn))`.
///
/// Two forms:
/// - `aggregate!(Agg, Rel, query_fn)` — direct function pointer
/// - `aggregate!(Agg, Rel, base_fn, axis)` — dispatches via axis match
#[macro_export]
macro_rules! aggregate {
    ($agg:ident, $rel:ident, $query:expr) => {{
        static LIST: $crate::FormulaList =
            $crate::FormulaList::Related($crate::MultiRelationship::$rel, $query);
        static F: $crate::Formula = $crate::Formula::Aggregate($crate::Aggregation::$agg, &LIST);
        &F
    }};
    ($agg:ident, $rel:ident, $base:path, $axis:expr) => {
        match $axis {
            $crate::Axis::Horizontal => {
                static LIST: $crate::FormulaList =
                    $crate::FormulaList::Related($crate::MultiRelationship::$rel, {
                        fn wrap(
                            sty: &dyn $crate::StylerAccess,
                        ) -> ::core::option::Option<&'static $crate::Formula> {
                            $base(sty, $crate::Axis::Horizontal)
                        }
                        wrap as $crate::QueryFn
                    });
                static F: $crate::Formula =
                    $crate::Formula::Aggregate($crate::Aggregation::$agg, &LIST);
                &F
            }
            $crate::Axis::Vertical => {
                static LIST: $crate::FormulaList =
                    $crate::FormulaList::Related($crate::MultiRelationship::$rel, {
                        fn wrap(
                            sty: &dyn $crate::StylerAccess,
                        ) -> ::core::option::Option<&'static $crate::Formula> {
                            $base(sty, $crate::Axis::Vertical)
                        }
                        wrap as $crate::QueryFn
                    });
                static F: $crate::Formula =
                    $crate::Formula::Aggregate($crate::Aggregation::$agg, &LIST);
                &F
            }
        }
    };
}

// ============================================================================
// Arithmetic — variadic chaining
// ============================================================================

/// Addition: `a + b`, or chained `a + b + c + ...`.
#[macro_export]
macro_rules! add {
    ($a:expr, $b:expr $(,)?) => {{
        static F: $crate::Formula =
            $crate::Formula::BinOp($crate::Operation::Add, $a, $b);
        &F
    }};
    ($a:expr, $b:expr, $($rest:expr),+ $(,)?) => {
        $crate::add!($crate::add!($a, $b), $($rest),+)
    };
}

/// Subtraction: `a - b`, or chained `a - b - c - ...`.
#[macro_export]
macro_rules! sub {
    ($a:expr, $b:expr $(,)?) => {{
        static F: $crate::Formula =
            $crate::Formula::BinOp($crate::Operation::Sub, $a, $b);
        &F
    }};
    ($a:expr, $b:expr, $($rest:expr),+ $(,)?) => {
        $crate::sub!($crate::sub!($a, $b), $($rest),+)
    };
}

/// Multiplication: `a * b`.
#[macro_export]
macro_rules! mul {
    ($a:expr, $b:expr $(,)?) => {{
        static F: $crate::Formula = $crate::Formula::BinOp($crate::Operation::Mul, $a, $b);
        &F
    }};
}

/// Division: `a / b`.
#[macro_export]
macro_rules! div {
    ($a:expr, $b:expr $(,)?) => {{
        static F: $crate::Formula = $crate::Formula::BinOp($crate::Operation::Div, $a, $b);
        &F
    }};
}

/// Maximum: `max(a, b)`.
#[macro_export]
macro_rules! max {
    ($a:expr, $b:expr $(,)?) => {{
        static F: $crate::Formula = $crate::Formula::BinOp($crate::Operation::Max, $a, $b);
        &F
    }};
}

/// Minimum: `min(a, b)`.
#[macro_export]
macro_rules! min {
    ($a:expr, $b:expr $(,)?) => {{
        static F: $crate::Formula = $crate::Formula::BinOp($crate::Operation::Min, $a, $b);
        &F
    }};
}

// ============================================================================
// Line-breaking aggregation
// ============================================================================

/// Line-breaking aggregate over children.
///
/// Groups children into lines by accumulating `item_main_size` per child,
/// breaking when the sum exceeds `available_main`. Then aggregates within
/// each line using `within_line_agg`, and across lines using `line_agg`.
///
/// ```ignore
/// line_aggregate!(
///     line_agg: Sum,           // across lines
///     within_line_agg: Max,    // within each line
///     item_main_size: basis_query,  // for line-breaking
///     item_value: cross_query,      // value to aggregate
///     available_main: parent_content_width_formula,
///     gap: column_gap_formula,
///     line_gap: row_gap_formula,
/// )
/// ```
#[macro_export]
macro_rules! line_aggregate {
    (
        line_agg: $line_agg:ident,
        within_line_agg: $within_agg:ident,
        item_main_size: $main_size:expr,
        item_value: $value:expr,
        available_main: $available:expr,
        gap: $gap:expr,
        line_gap: $line_gap:expr $(,)?
    ) => {{
        static PARAMS: $crate::LineAggregateParams = $crate::LineAggregateParams {
            line_agg: $crate::Aggregation::$line_agg,
            within_line_agg: $crate::Aggregation::$within_agg,
            item_main_size: $main_size,
            item_value: $value,
            available_main: $available,
            gap: $gap,
            line_gap: $line_gap,
        };
        static F: $crate::Formula = $crate::Formula::LineAggregate(PARAMS);
        &F
    }};
}

/// Aggregate over all lines before the current item's line.
///
/// Computes line assignments, finds which line the current item is on,
/// then aggregates values from all previous lines. Within each previous
/// line, values are combined using `within_line_agg`; across lines,
/// the per-line results are combined using `line_agg`. Line gaps are
/// added between previous lines.
///
/// ```ignore
/// prev_lines_aggregate!(
///     line_agg: Sum,           // across previous lines
///     within_line_agg: Max,    // within each previous line
///     item_main_size: basis_query,  // for line-breaking
///     item_value: cross_query,      // value to aggregate
///     available_main: parent_content_width_formula,
///     gap: column_gap_formula,
///     line_gap: row_gap_formula,
/// )
/// ```
#[macro_export]
macro_rules! prev_lines_aggregate {
    (
        line_agg: $line_agg:ident,
        within_line_agg: $within_agg:ident,
        item_main_size: $main_size:expr,
        item_value: $value:expr,
        available_main: $available:expr,
        gap: $gap:expr,
        line_gap: $line_gap:expr $(,)?
    ) => {{
        static PARAMS: $crate::PrevLinesAggregateParams = $crate::PrevLinesAggregateParams {
            line_agg: $crate::Aggregation::$line_agg,
            within_line_agg: $crate::Aggregation::$within_agg,
            item_main_size: $main_size,
            item_value: $value,
            available_main: $available,
            gap: $gap,
            line_gap: $line_gap,
        };
        static F: $crate::Formula = $crate::Formula::PrevLinesAggregate(PARAMS);
        &F
    }};
}

/// Line-aware aggregate over same-line siblings.
///
/// Like `aggregate!`, but only considers siblings on the same flex/inline line
/// as the current item. Line assignments are computed using the same parameters
/// as `line_aggregate!`.
///
/// ```ignore
/// line_item_aggregate!(
///     agg: Sum,
///     rel: PrevSiblings,
///     query: size_query,
///     item_main_size: basis_query,
///     available_main: parent_content_width_formula,
///     gap: column_gap_formula,
/// )
/// ```
#[macro_export]
macro_rules! line_item_aggregate {
    (
        agg: $agg:ident,
        rel: $rel:ident,
        query: $query:expr,
        item_main_size: $main_size:expr,
        available_main: $available:expr,
        gap: $gap:expr $(,)?
    ) => {{
        static PARAMS: $crate::LineItemAggregateParams = $crate::LineItemAggregateParams {
            agg: $crate::Aggregation::$agg,
            relationship: $crate::MultiRelationship::$rel,
            query: $query,
            item_main_size: $main_size,
            available_main: $available,
            gap: $gap,
        };
        static F: $crate::Formula = $crate::Formula::LineItemAggregate(PARAMS);
        &F
    }};
}

// ============================================================================
// Imperative resolution
// ============================================================================

/// Imperative formula: delegates to a Rust function for computation that
/// cannot be expressed declaratively (e.g., iterative algorithms).
///
/// The function receives the current node, its styler, and a `resolve`
/// callback for evaluating sub-formulas. It returns results for multiple
/// nodes (batch caching).
///
/// ```ignore
/// imperative!(my_resolver_fn)
/// ```
#[macro_export]
macro_rules! imperative {
    ($fn:expr) => {{
        static F: $crate::Formula = $crate::Formula::Imperative($fn);
        &F
    }};
}
