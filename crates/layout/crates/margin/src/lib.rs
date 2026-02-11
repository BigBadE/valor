//! Margin computation using the formula system.

use lightningcss::properties::PropertyId;
use rewrite_core::{Edge, Formula};

/// Compute margin formula for an edge.
pub fn margin(edge: Edge) -> &'static Formula {
    match edge {
        Edge::Top => {
            static RESULT: Formula = Formula::CssValue(PropertyId::MarginTop);
            &RESULT
        }
        Edge::Right => {
            static RESULT: Formula = Formula::CssValue(PropertyId::MarginRight);
            &RESULT
        }
        Edge::Bottom => {
            static RESULT: Formula = Formula::CssValue(PropertyId::MarginBottom);
            &RESULT
        }
        Edge::Left => {
            static RESULT: Formula = Formula::CssValue(PropertyId::MarginLeft);
            &RESULT
        }
    }
}

/// Compute collapsed margin between two elements.
pub fn collapsed_margin(edge: Edge) -> &'static Formula {
    // Margin collapsing: max of adjacent margins
    // For now, just return the margin value
    margin(edge)
}
