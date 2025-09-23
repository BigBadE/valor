//! Root-level visual formatting helpers
//! Spec: <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>

use css_box::compute_box_sides;
use style_engine::ComputedStyle;

use super::vertical::effective_child_top_margin;
use crate::{ContainerMetrics, LayoutNodeKind, Layouter, box_tree};
use js::NodeKey;

/// Compute the root y position after collapsing the parent's top margin with the
/// first child's top margin when eligible (CSS 2.2 §8.3.1 parent–first-child collapse).
#[inline]
pub fn compute_root_y_after_top_collapse(
    layouter: &Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
) -> i32 {
    if metrics.padding_top == 0i32 && metrics.border_top == 0i32 {
        let flattened =
            box_tree::flatten_display_children(&layouter.children, &layouter.computed_styles, root);
        if let Some(first_child) = flattened
            .into_iter()
            .find(|key| matches!(layouter.nodes.get(key), Some(&LayoutNodeKind::Block { .. })))
        {
            let first_style = layouter
                .computed_styles
                .get(&first_child)
                .cloned()
                .unwrap_or_else(ComputedStyle::default);
            let first_sides = compute_box_sides(&first_style);
            let first_effective_top =
                effective_child_top_margin(layouter, first_child, &first_sides);
            let collapsed =
                Layouter::collapse_margins_pair(metrics.margin_top, first_effective_top);
            return collapsed.max(0i32);
        }
    }
    metrics.margin_top
}
