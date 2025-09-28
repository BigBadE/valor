//! Inline baseline provider hook for core.
//! Spec: CSS Inline Layout — Baseline alignment plumbing (provider-based)

extern crate alloc;

use crate::Layouter;
use alloc::sync::Arc;
use js::NodeKey;
use once_cell::sync::OnceCell;
use css_display::inline_context::build_inline_context_with_filter;
use css_text::{collapse_whitespace, default_line_height_px};
use css_orchestrator::style_model::ComputedStyle;
use core::hash::BuildHasher; // for HashMap hasher bound used by build_inline_context
use std::collections::HashMap;

/// Provider API for first/last baselines in CSS px for a given node.
/// Implementations can use inline layout and text shaping to compute true baselines.
/// Provider trait to compute first/last baselines for a node.
/// Returned values are CSS pixels from the border-box top.
pub trait InlineBaselineProvider: Send + Sync {
    fn baselines(&self, layouter: &Layouter, node: NodeKey) -> Option<(f32, f32)>;
}

/// Global registration point for an inline baseline provider.
static BASELINE_PROVIDER: OnceCell<Arc<dyn InlineBaselineProvider>> = OnceCell::new();

/// Register a global inline baseline provider.
/// Returns true on first successful registration; false if a provider was already set.
/// Call this from your inline/text engine initialization.
#[inline]
pub fn set_inline_baseline_provider<P>(provider: P) -> bool
where
    P: InlineBaselineProvider + 'static,
{
    BASELINE_PROVIDER.set(Arc::new(provider)).is_ok()
}

/// Get the registered baseline provider, if any.
#[inline]
pub fn get_inline_baseline_provider() -> Option<Arc<dyn InlineBaselineProvider>> {
    BASELINE_PROVIDER.get().cloned()
}

/// A simple text/inline baseline provider using inline context grouping and default line height.
///
/// Heuristic behavior:
/// - Builds line boxes by grouping contiguous inline-level children, skipping ignorable whitespace.
/// - First baseline = default line-height (based on node style) in px.
/// - Last baseline = number_of_lines * default line-height in px.
struct DefaultTextBaselineProvider;

impl InlineBaselineProvider for DefaultTextBaselineProvider {
    fn baselines(&self, layouter: &Layouter, node: NodeKey) -> Option<(f32, f32)> {
        // Gather flat children
        let children = layouter.children.get(&node).cloned().unwrap_or_default();
        if children.is_empty() {
            return None;
        }
        // Build a quick lookup for node kinds to detect ignorable whitespace text runs
        let mut kind_map: HashMap<NodeKey, crate::LayoutNodeKind> = HashMap::new();
        for (k, kind, _kids) in layouter.snapshot() {
            kind_map.insert(k, kind);
        }
        let styles = &layouter.computed_styles;
        let parent_style: Option<&ComputedStyle> = styles.get(&node);
        let skip_predicate = |n: NodeKey| -> bool {
            match kind_map.get(&n) {
                Some(crate::LayoutNodeKind::InlineText { text }) => collapse_whitespace(text).is_empty(),
                _ => false,
            }
        };
        let lines = build_inline_context_with_filter(&children, styles, parent_style, skip_predicate);
        if lines.is_empty() {
            return None;
        }
        // Use the node's own style (or default) to estimate line height
        let style = styles.get(&node).cloned().unwrap_or_else(ComputedStyle::default);
        let lh_px = default_line_height_px(&style) as f32;
        let first = lh_px.max(0.0);
        let last = (lines.len() as f32 * lh_px).max(first);
        Some((first, last))
    }
}

/// Register the default text baseline provider if none is set yet. Returns true if registration is active.
#[inline]
pub fn register_default_text_baseline_provider() -> bool {
    if get_inline_baseline_provider().is_some() {
        return true;
    }
    set_inline_baseline_provider(DefaultTextBaselineProvider)
}
