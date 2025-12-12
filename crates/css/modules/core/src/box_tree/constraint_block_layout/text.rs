//! Text layout and measurement for block layout.

use super::ConstraintLayoutTree;
use super::shared::ChildrenLayoutState;
use css_box::LayoutUnit;
use css_text::default_line_height_px;
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

        let (text_width, text_height) =
            self.measure_text(child, Some(parent), child_space.available_inline_size);

        // Get the parent's line-height for vertical spacing
        let parent_style = self.styles.get(&parent).cloned().unwrap_or_default();
        let line_height = parent_style
            .line_height
            .unwrap_or_else(|| default_line_height_px(&parent_style) as f32);

        let text_result = LayoutResult {
            inline_size: text_width,
            block_size: text_height, // Text node rect uses actual font height
            bfc_offset: BfcOffset::new(child_space.bfc_offset.inline_offset, Some(resolved_offset)),
            exclusion_space: child_space.exclusion_space.clone(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        };

        self.layout_results.insert(child, text_result);
        child_space.margin_strut = MarginStrut::default();
        // Use line-height for vertical spacing, not text_height
        child_space.bfc_offset.block_offset =
            Some(resolved_offset + LayoutUnit::from_px(line_height.round()));
        true
    }

    /// Measure text node dimensions using actual font metrics.
    pub(super) fn measure_text(
        &self,
        child_node: NodeKey,
        parent_node: Option<NodeKey>,
        available_inline: AvailableSize,
    ) -> (f32, f32) {
        let text = self.text_nodes.get(&child_node).map_or("", String::as_str);

        if text.is_empty() {
            return (0.0, 0.0);
        }

        // Text nodes inherit font properties from their parent
        // Try to get parent's style, fallback to default
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
                    log::debug!(
                        "measure_text: FITS for text='{}', available={}, metrics.width={}, using natural width",
                        text,
                        available_width,
                        metrics.width
                    );
                    (metrics.width, metrics.height)
                } else {
                    // Text needs wrapping - measure wrapped height and use available width
                    let (height, _line_count) = measure_text_wrapped(text, &style, available_width);
                    log::debug!(
                        "measure_text: WRAPPING for text='{}', available={}, metrics.width={}, wrapped_height={}",
                        text,
                        available_width,
                        metrics.width,
                        height
                    );
                    (available_width, height)
                }
            }
        }
    }
}
