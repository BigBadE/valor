//! Provider implementations for size and offset queries.

use crate::formula_trait::{OffsetFormulaProvider, SizeFormulaProvider};
use rewrite_core::*;

/// Complete provider implementation that handles all layout modes.
pub struct LayoutProvider;

impl SizeFormulaProvider for LayoutProvider {
    fn size_formula(&self, _scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
        // Simple implementation: return CSS size property
        // TODO: Dispatch based on display mode
        match axis {
            Axis::Horizontal => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Size(Axis::Horizontal));
                &RESULT
            }
            Axis::Vertical => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Size(Axis::Vertical));
                &RESULT
            }
        }
    }

    fn padding_formula(&self, _scoped: &mut ScopedDb, edge: Edge) -> &'static Formula {
        match edge {
            Edge::Top => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Padding(Edge::Top));
                &RESULT
            }
            Edge::Right => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Padding(Edge::Right));
                &RESULT
            }
            Edge::Bottom => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Padding(Edge::Bottom));
                &RESULT
            }
            Edge::Left => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Padding(Edge::Left));
                &RESULT
            }
        }
    }

    fn border_formula(&self, _scoped: &mut ScopedDb, edge: Edge) -> &'static Formula {
        match edge {
            Edge::Top => {
                static RESULT: Formula = Formula::Value(CssValueProperty::BorderWidth(Edge::Top));
                &RESULT
            }
            Edge::Right => {
                static RESULT: Formula = Formula::Value(CssValueProperty::BorderWidth(Edge::Right));
                &RESULT
            }
            Edge::Bottom => {
                static RESULT: Formula =
                    Formula::Value(CssValueProperty::BorderWidth(Edge::Bottom));
                &RESULT
            }
            Edge::Left => {
                static RESULT: Formula = Formula::Value(CssValueProperty::BorderWidth(Edge::Left));
                &RESULT
            }
        }
    }

    fn margin_formula(&self, _scoped: &mut ScopedDb, edge: Edge) -> &'static Formula {
        match edge {
            Edge::Top => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Margin(Edge::Top));
                &RESULT
            }
            Edge::Right => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Margin(Edge::Right));
                &RESULT
            }
            Edge::Bottom => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Margin(Edge::Bottom));
                &RESULT
            }
            Edge::Left => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Margin(Edge::Left));
                &RESULT
            }
        }
    }
}

impl OffsetFormulaProvider for LayoutProvider {
    fn offset_formula(&self, _scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
        match axis {
            Axis::Horizontal => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Offset(Edge::Left));
                &RESULT
            }
            Axis::Vertical => {
                static RESULT: Formula = Formula::Value(CssValueProperty::Offset(Edge::Top));
                &RESULT
            }
        }
    }
}
