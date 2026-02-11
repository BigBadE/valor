//! Formatting context detection queries.
//!
//! Formatting contexts are natural parallelism boundaries in layout.
//! Changes inside a formatting context rarely affect nodes outside it.

use css_orchestrator::style_model::{Display, Float, Overflow, Position};
use js::NodeKey;
use valor_query::{Query, QueryDatabase};

/// Type of formatting context established by a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormattingContextType {
    /// Block Formatting Context (normal flow blocks)
    Block,

    /// Inline Formatting Context (text and inline boxes)
    Inline,

    /// Flex Formatting Context
    Flex,

    /// Grid Formatting Context
    Grid,

    /// No formatting context (e.g., display: none, display: contents)
    None,
}

/// Query to determine what type of formatting context a node establishes.
pub struct FormattingContextQuery;

impl Query for FormattingContextQuery {
    type Key = NodeKey;
    type Value = FormattingContextType;

    fn execute(db: &QueryDatabase, key: Self::Key) -> Self::Value {
        // Get the computed style for this node via query (not input)
        use css_orchestrator::queries::ComputedStyleQuery;
        let style = db.query::<ComputedStyleQuery>(key);

        // display: none doesn't establish any formatting context
        if matches!(style.display, Display::None) {
            return FormattingContextType::None;
        }

        // display: contents doesn't establish a formatting context
        if matches!(style.display, Display::Contents) {
            return FormattingContextType::None;
        }

        // Flex containers establish flex formatting context
        if matches!(style.display, Display::Flex | Display::InlineFlex) {
            return FormattingContextType::Flex;
        }

        // Grid containers establish grid formatting context
        if matches!(style.display, Display::Grid | Display::InlineGrid) {
            return FormattingContextType::Grid;
        }

        // Check if this establishes a BFC
        let establishes_bfc = is_bfc_root(&style);

        if establishes_bfc {
            FormattingContextType::Block
        } else {
            // Participate in parent's formatting context
            FormattingContextType::None
        }
    }
}

/// Check if a style establishes a Block Formatting Context.
fn is_bfc_root(style: &css_orchestrator::style_model::ComputedStyle) -> bool {
    // Floats establish BFC
    if !matches!(style.float, Float::None) {
        return true;
    }

    // Overflow other than visible establishes BFC
    if !matches!(style.overflow, Overflow::Visible) {
        return true;
    }

    // Absolutely positioned elements establish BFC
    if matches!(style.position, Position::Absolute | Position::Fixed) {
        return true;
    }

    false
}
