//! Margin collapsing re-exports.
//!
//! This module re-exports margin collapsing functionality from the margin crate.
//! The implementation has been moved to avoid circular dependencies.

// Re-export all public functions from the margin crate
pub use rewrite_layout_margin::{
    get_effective_margin_end, get_effective_margin_start, get_margin_for_offset,
};

// These were previously exported but are now internal to the margin crate
// If needed externally, they can be re-exported from the margin crate
use rewrite_core::ScopedDb;
use rewrite_css::Subpixels;
use rewrite_css::{EndMarker, StartMarker};
use rewrite_css_dimensional::MarginQuery;

/// Compute collapsed margin for the start edge - now delegated to margin crate.
pub fn compute_collapsed_margin_start(scoped: &mut ScopedDb) -> Subpixels {
    get_effective_margin_start(scoped)
}

/// Compute collapsed margin for the end edge - now delegated to margin crate.
pub fn compute_collapsed_margin_end(scoped: &mut ScopedDb) -> Subpixels {
    get_effective_margin_end(scoped)
}

/// Check if element can collapse with parent - simplified wrapper.
pub fn can_collapse_with_parent_start(_scoped: &mut ScopedDb) -> bool {
    // This is disabled in the margin crate implementation
    false
}
