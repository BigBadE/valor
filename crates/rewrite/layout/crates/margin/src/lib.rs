//! Margin computation using the formula system.

use rewrite_core::*;

/// Compute margin formula for an edge.
pub fn margin(_scoped: &mut ScopedDb, edge: Edge) -> &'static Formula {
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

/// Compute collapsed margin between two elements.
pub fn collapsed_margin(_scoped: &mut ScopedDb, edge: Edge) -> &'static Formula {
    // Margin collapsing: max of adjacent margins
    // For now, just return the margin value
    margin(_scoped, edge)
}
