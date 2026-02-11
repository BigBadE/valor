//! CSS stacking context and z-index paint order implementation.
//!
//! Implements CSS 2.2 §9.9.1 stacking order and CSS 2.2 Appendix E paint order.
//! Handles z-index layering, opacity groups, and proper CSS paint ordering.

use core::cmp::Ordering;
use css::style_types::{ComputedStyle, Position};
use js::NodeKey;
use renderer::{DisplayItem, StackingContextBoundary};
use std::collections::HashMap;

/// Categories for CSS stacking order (CSS 2.2 §9.9.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StackingLayer {
    /// Negative z-index stacking contexts (painted first)
    NegativeZIndex,
    /// In-flow non-positioned block-level descendants
    BlockBackground,
    /// Non-positioned floats (not yet implemented)
    _Floats,
    /// Inline-level descendants (not yet implemented)
    _InlineLevel,
    /// Positioned descendants with z-index: auto/0
    PositionedAutoZero,
    /// Positive z-index stacking contexts (painted last)
    PositiveZIndex,
}

/// A child node with its stacking information
#[derive(Debug, Clone)]
pub struct StackingChild {
    /// The node key identifying this child
    pub key: NodeKey,
    /// The stacking layer this child belongs to
    pub layer: StackingLayer,
    /// The z-index value (if applicable)
    pub z_index: i32,
}

impl StackingChild {
    /// Determine the stacking layer for a node based on its computed style
    pub const fn from_style(key: NodeKey, style: &ComputedStyle) -> Self {
        // Check if this element creates a stacking context with z-index
        let is_positioned = matches!(
            style.position,
            Position::Relative | Position::Absolute | Position::Fixed
        );

        let (layer, z_index) = if is_positioned {
            if let Some(z_val) = style.z_index {
                if z_val < 0i32 {
                    (StackingLayer::NegativeZIndex, z_val)
                } else if z_val > 0i32 {
                    (StackingLayer::PositiveZIndex, z_val)
                } else {
                    (StackingLayer::PositionedAutoZero, z_val)
                }
            } else {
                // Positioned with z-index: auto
                (StackingLayer::PositionedAutoZero, 0i32)
            }
        } else {
            // Non-positioned elements use block background layer
            (StackingLayer::BlockBackground, 0i32)
        };

        Self {
            key,
            layer,
            z_index,
        }
    }

    /// Compare two stacking children for paint order
    pub fn compare_paint_order(&self, other: &Self) -> Ordering {
        // First sort by layer
        match self.layer.cmp(&other.layer) {
            Ordering::Equal => {
                // Within same layer, sort by z-index
                match self.layer {
                    StackingLayer::NegativeZIndex => {
                        // Most negative first
                        self.z_index.cmp(&other.z_index)
                    }
                    StackingLayer::PositiveZIndex => {
                        // Least positive first
                        self.z_index.cmp(&other.z_index)
                    }
                    _ => Ordering::Equal, // Preserve DOM order for same layer
                }
            }
            ordering => ordering,
        }
    }
}

/// Sort children into CSS stacking order.
/// Children without computed styles (like text nodes) are included in `BlockBackground` layer.
pub fn sort_children_by_stacking_order(
    children: &[NodeKey],
    styles: &HashMap<NodeKey, ComputedStyle>,
) -> Vec<StackingChild> {
    let mut stacking_children: Vec<StackingChild> = children
        .iter()
        .map(|key| {
            // Get style if it exists, otherwise use default (for text nodes, etc.)
            styles.get(key).map_or_else(
                || {
                    // Text nodes and other non-styled nodes go in BlockBackground layer
                    StackingChild {
                        key: *key,
                        layer: StackingLayer::BlockBackground,
                        z_index: 0,
                    }
                },
                |style| StackingChild::from_style(*key, style),
            )
        })
        .collect();

    // Stable sort to preserve DOM order within same layer
    stacking_children.sort_by(StackingChild::compare_paint_order);

    stacking_children
}

/// Determine if a node creates a stacking context boundary that needs display items.
///
/// Note: Z-index creates stacking contexts for CSS purposes, but doesn't need display
/// items because paint order handles the layering. Only opacity needs offscreen rendering.
pub fn creates_stacking_context(style: &ComputedStyle) -> Option<StackingContextBoundary> {
    // Opacity < 1.0 creates a stacking context that needs offscreen compositing
    if let Some(alpha) = style.opacity
        && alpha < 1.0f32
    {
        return Some(StackingContextBoundary::Opacity { alpha });
    }

    // Z-index creates stacking contexts for CSS purposes, but we handle ordering
    // via paint order in sort_children_by_stacking_order, not via display items.
    // The wgpu backend doesn't need ZIndex markers for rendering.

    None
}

/// Emit display items for a stacking context boundary
pub fn emit_stacking_context<PaintFn: FnOnce(&mut Vec<DisplayItem>)>(
    boundary: StackingContextBoundary,
    items: &mut Vec<DisplayItem>,
    paint_content: PaintFn,
) {
    items.push(DisplayItem::BeginStackingContext { boundary });
    paint_content(items);
    items.push(DisplayItem::EndStackingContext);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that negative z-index is painted before positive z-index.
    ///
    /// # Panics
    /// Panics if the layer ordering is incorrect.
    #[test]
    fn stacking_order_negative_before_positive() {
        let style_neg = ComputedStyle {
            position: Position::Absolute,
            z_index: Some(-1i32),
            ..Default::default()
        };

        let style_pos = ComputedStyle {
            position: Position::Absolute,
            z_index: Some(1i32),
            ..Default::default()
        };

        let child_neg = StackingChild::from_style(NodeKey(1), &style_neg);
        let child_pos = StackingChild::from_style(NodeKey(2), &style_pos);

        assert_eq!(child_neg.layer, StackingLayer::NegativeZIndex);
        assert_eq!(child_pos.layer, StackingLayer::PositiveZIndex);
        assert!(child_neg.layer < child_pos.layer);
    }

    /// Test that static elements are painted before positioned elements.
    ///
    /// # Panics
    /// Panics if the layer ordering is incorrect.
    #[test]
    fn stacking_order_static_vs_positioned() {
        let style_static = ComputedStyle {
            position: Position::Static,
            ..Default::default()
        };

        let style_positioned = ComputedStyle {
            position: Position::Absolute,
            z_index: Some(0i32),
            ..Default::default()
        };

        let child_static = StackingChild::from_style(NodeKey(1), &style_static);
        let child_positioned = StackingChild::from_style(NodeKey(2), &style_positioned);

        assert_eq!(child_static.layer, StackingLayer::BlockBackground);
        assert_eq!(child_positioned.layer, StackingLayer::PositionedAutoZero);
        assert!(child_static.layer < child_positioned.layer);
    }
}
