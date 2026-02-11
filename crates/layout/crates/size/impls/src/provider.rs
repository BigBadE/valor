//! Provider implementations for size and offset queries.

use crate::formula_trait::{OffsetFormulaProvider, SizeFormulaProvider};
use lightningcss::properties::PropertyId;
use rewrite_core::{Axis, Edge, Formula};

/// Complete provider implementation that handles all layout modes.
pub struct LayoutProvider;

impl SizeFormulaProvider for LayoutProvider {
    fn size_formula(&self, axis: Axis) -> &'static Formula {
        // Simple implementation: return CSS size property
        // TODO: Dispatch based on display mode
        match axis {
            Axis::Horizontal => {
                static RESULT: Formula = Formula::CssValue(PropertyId::Width);
                &RESULT
            }
            Axis::Vertical => {
                static RESULT: Formula = Formula::CssValue(PropertyId::Height);
                &RESULT
            }
        }
    }

    fn padding_formula(&self, edge: Edge) -> &'static Formula {
        match edge {
            Edge::Top => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::PaddingTop, 0);
                &RESULT
            }
            Edge::Right => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::PaddingRight, 0);
                &RESULT
            }
            Edge::Bottom => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::PaddingBottom, 0);
                &RESULT
            }
            Edge::Left => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::PaddingLeft, 0);
                &RESULT
            }
        }
    }

    fn border_formula(&self, edge: Edge) -> &'static Formula {
        match edge {
            Edge::Top => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::BorderTopWidth, 0);
                &RESULT
            }
            Edge::Right => {
                static RESULT: Formula =
                    Formula::CssValueOrDefault(PropertyId::BorderRightWidth, 0);
                &RESULT
            }
            Edge::Bottom => {
                static RESULT: Formula =
                    Formula::CssValueOrDefault(PropertyId::BorderBottomWidth, 0);
                &RESULT
            }
            Edge::Left => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::BorderLeftWidth, 0);
                &RESULT
            }
        }
    }

    fn margin_formula(&self, edge: Edge) -> &'static Formula {
        match edge {
            Edge::Top => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::MarginTop, 0);
                &RESULT
            }
            Edge::Right => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::MarginRight, 0);
                &RESULT
            }
            Edge::Bottom => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::MarginBottom, 0);
                &RESULT
            }
            Edge::Left => {
                static RESULT: Formula = Formula::CssValueOrDefault(PropertyId::MarginLeft, 0);
                &RESULT
            }
        }
    }
}

impl OffsetFormulaProvider for LayoutProvider {
    fn offset_formula(&self, axis: Axis) -> &'static Formula {
        match axis {
            Axis::Horizontal => {
                static RESULT: Formula = Formula::CssValue(PropertyId::Left);
                &RESULT
            }
            Axis::Vertical => {
                static RESULT: Formula = Formula::CssValue(PropertyId::Top);
                &RESULT
            }
        }
    }
}
