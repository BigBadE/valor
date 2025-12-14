//! Text layout and measurement for block layout.

use super::ConstraintLayoutTree;
use super::shared::ChildrenLayoutState;
use css_box::LayoutUnit;
use css_text::measurement::{measure_text, measure_text_wrapped};
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::super::margin_strut::MarginStrut;

impl ConstraintLayoutTree {
    pub(super) fn layout_text_child(
        &mut self,
        parent_child: (NodeKey, NodeKey),
        child_space: &mut ConstraintSpace,
        state: &mut ChildrenLayoutState,
        can_collapse_with_children: bool,
    ) -> bool {
        let (parent, child) = parent_child;
        let text = self.text_nodes.get(&child).map_or("", String::as_str);
        let is_whitespace_only = text.chars().all(char::is_whitespace);

        if is_whitespace_only {
            let text_result = LayoutResult {
                inline_size: 0.0,
                block_size: 0.0,
                bfc_offset: child_space.bfc_offset,
                exclusion_space: child_space.exclusion_space.clone(),
                end_margin_strut: MarginStrut::default(),
                baseline: None,
                needs_relayout: false,
            };
            self.layout_results.insert(child, text_result);
            return false;
        }

        // Check if this is a continuation of a previous text node.
        // Continuation text nodes get positioned but with zero inline size,
        // so rendering can find them and combine their text with the lead node.
        let is_continuation = self.children.get(&parent).is_some_and(|siblings| {
            siblings
                .iter()
                .position(|&sibling| sibling == child)
                .is_some_and(|child_index| {
                    child_index > 0 && {
                        let prev_sibling = siblings[child_index - 1];
                        self.text_nodes.contains_key(&prev_sibling)
                    }
                })
        });

        state.has_text_content = true;

        // Text nodes don't have margins, but if parent can collapse with children,
        // we need to collapse the parent's margin in the strut and place text after it.
        let mut resolved_offset = child_space
            .bfc_offset
            .block_offset
            .unwrap_or(LayoutUnit::zero());

        if !state.first_inflow_child_seen && can_collapse_with_children {
            // This is the first child and parent can collapse with children.
            // The margin strut contains the parent's margin. Since text has no margin,
            // we collapse the strut and add it to position the text.
            let collapsed_margin = child_space.margin_strut.collapse();
            resolved_offset += collapsed_margin;

            // Update parent's resolved position to match first child
            state.resolved_bfc_offset.block_offset = Some(resolved_offset);
            state.first_inflow_child_seen = true;
        }

        // For the lead text node, measure the combined text.
        // For continuation nodes, they contribute to the combined measurement but
        // get zero inline size individually (the lead node takes all the space).
        let (text_width, text_height) = if is_continuation {
            // Continuation node: zero inline size, same block size as lead
            // The rendering pass will combine all consecutive text nodes
            (0.0, 0.0)
        } else {
            self.measure_text(child, Some(parent), child_space.available_inline_size)
        };

        let text_result = LayoutResult {
            inline_size: text_width,
            block_size: text_height,
            bfc_offset: BfcOffset::new(child_space.bfc_offset.inline_offset, Some(resolved_offset)),
            exclusion_space: child_space.exclusion_space.clone(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        };

        // Only store layout results during final layout, not during measurement passes.
        // During grid/flex sizing measurement, text gets laid out with intrinsic widths,
        // but we need to recalculate with the final definite widths from the container.
        if !child_space.is_for_measurement_only {
            self.layout_results.insert(child, text_result);
        }

        // Only advance BFC offset for the lead text node, not continuations
        if !is_continuation {
            child_space.margin_strut = MarginStrut::default();
            // Advance BFC offset by the actual text height (which includes wrapping)
            // Note: text_height already accounts for multiple lines when text wraps
            child_space.bfc_offset.block_offset =
                Some(resolved_offset + LayoutUnit::from_px(text_height));
        }

        !is_continuation
    }

    /// Combine consecutive text nodes for a given child node.
    fn combine_consecutive_text_nodes(&self, child_node: NodeKey, siblings: &[NodeKey]) -> String {
        let mut combined = String::new();
        let mut found_start = false;

        for &sibling in siblings {
            if sibling == child_node {
                found_start = true;
            }

            if !found_start {
                continue;
            }

            let Some(text) = self.text_nodes.get(&sibling) else {
                // Stop at first non-text node
                break;
            };
            combined.push_str(text);
        }

        combined
    }

    /// Measure text node dimensions using actual font metrics.
    /// For consecutive text nodes, combines them before measuring to ensure proper wrapping.
    pub(super) fn measure_text(
        &self,
        child_node: NodeKey,
        parent_node: Option<NodeKey>,
        available_inline: AvailableSize,
    ) -> (f32, f32) {
        // Combine consecutive text nodes that share the same parent
        // This ensures "Unicode ", "&", " Special Characters" are measured together
        let combined_text = parent_node.map_or_else(
            || {
                self.text_nodes
                    .get(&child_node)
                    .map_or(String::new(), String::clone)
            },
            |parent| {
                self.children.get(&parent).map_or_else(
                    || {
                        self.text_nodes
                            .get(&child_node)
                            .map_or(String::new(), String::clone)
                    },
                    |siblings| self.combine_consecutive_text_nodes(child_node, siblings),
                )
            },
        );

        let text = combined_text.as_str();

        if text.is_empty() {
            return (0.0, 0.0);
        }

        // Text nodes inherit font properties from their parent.
        // Get the parent's COMPUTED style (which includes cascaded inline styles).
        let style = parent_node.map_or_else(
            || self.styles.get(&child_node).cloned().unwrap_or_default(),
            |parent| self.styles.get(&parent).cloned().unwrap_or_default(),
        );

        // Measure without wrapping first to see if text fits
        let metrics = measure_text(text, &style);

        // For intrinsic sizing (Indefinite), always use the natural text width without wrapping
        // For definite sizes, check if wrapping is needed
        // IMPORTANT: If available width is absurdly small (< 50px), it's likely an intrinsic sizing
        // pass with incorrect ICB width, so use natural width instead of wrapping
        match available_inline {
            AvailableSize::Indefinite | AvailableSize::MaxContent | AvailableSize::MinContent => {
                // Intrinsic sizing - use natural width without wrapping
                log::debug!(
                    "measure_text: INTRINSIC sizing for text='{}', mode={:?}, metrics.width={}, using natural width",
                    text,
                    available_inline,
                    metrics.width
                );
                (metrics.width, metrics.height)
            }
            AvailableSize::Definite(size) => {
                const MIN_WRAP_WIDTH: f32 = 50.0;
                let available_width = size.to_px();
                // Don't wrap at absurdly small widths - use natural width instead
                if available_width < MIN_WRAP_WIDTH {
                    log::debug!(
                        "measure_text: SMALL DEFINITE ({}px < {}px) for text='{}', metrics.width={}, using natural width",
                        available_width,
                        MIN_WRAP_WIDTH,
                        text,
                        metrics.width
                    );
                    (metrics.width, metrics.height)
                } else if metrics.width <= available_width {
                    // Text fits on one line - use actual width
                    (metrics.width, metrics.height)
                } else {
                    // Text needs wrapping - measure wrapped height and use available width
                    let (height, _line_count) = measure_text_wrapped(text, &style, available_width);
                    (available_width, height)
                }
            }
        }
    }
}
