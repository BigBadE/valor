//! Display list builder from layout tree.
//!
//! Converts layout rectangles and computed styles into a `DisplayList`
//! following correct CSS paint order.

use crate::display_list::{DisplayItem, DisplayList};
use crate::paint::stacking::{StackingContext, StackingLevel};
use crate::paint::traversal::{NodeId, PaintNode, traverse_paint_tree};
use std::collections::HashMap;

/// Layout rectangle with position and size.
#[derive(Debug, Clone, Copy)]
pub struct LayoutRect {
    /// X coordinate in pixels.
    pub x: f32,
    /// Y coordinate in pixels.
    pub y: f32,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
}

/// Computed style properties relevant for painting.
#[derive(Debug, Clone)]
pub struct PaintStyle {
    /// Background color (RGBA).
    pub background_color: [f32; 4],
    /// Text color (RGB).
    pub text_color: [f32; 3],
    /// Font size in pixels.
    pub font_size: f32,
    /// Opacity (0.0 = transparent, 1.0 = opaque).
    pub opacity: f32,
    /// Z-index for positioned elements.
    pub z_index: Option<i32>,
    /// Whether element is positioned.
    pub is_positioned: bool,
    /// Overflow clipping bounds.
    pub overflow_clip: Option<(f32, f32, f32, f32)>,
}

impl Default for PaintStyle {
    fn default() -> Self {
        Self {
            background_color: [0.0, 0.0, 0.0, 0.0],
            text_color: [0.0, 0.0, 0.0],
            font_size: 16.0,
            opacity: 1.0,
            z_index: None,
            is_positioned: false,
            overflow_clip: None,
        }
    }
}

/// Layout node kind.
#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    /// Block-level box.
    Block,
    /// Inline-level text.
    InlineText { text: String },
    /// Positioned element.
    Positioned,
}

/// Builder for creating display lists from layout tree.
pub struct DisplayListBuilder {
    /// Layout rectangles by node ID.
    rects: HashMap<NodeId, LayoutRect>,
    /// Computed styles by node ID.
    styles: HashMap<NodeId, PaintStyle>,
    /// Node kinds by node ID.
    kinds: HashMap<NodeId, LayoutNodeKind>,
    /// Paint tree structure.
    nodes: HashMap<NodeId, PaintNode>,
}

impl DisplayListBuilder {
    /// Create a new display list builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rects: HashMap::new(),
            styles: HashMap::new(),
            kinds: HashMap::new(),
            nodes: HashMap::new(),
        }
    }

    /// Add a layout node to the builder.
    pub fn add_node(
        &mut self,
        id: NodeId,
        parent: Option<NodeId>,
        rect: LayoutRect,
        style: PaintStyle,
        kind: LayoutNodeKind,
    ) {
        self.rects.insert(id, rect);
        self.styles.insert(id, style.clone());
        self.kinds.insert(id, kind);

        // Determine stacking level
        let level = if style.is_positioned {
            if let Some(z) = style.z_index {
                StackingLevel::from_z_index(z)
            } else {
                StackingLevel::PositionedZeroOrAuto
            }
        } else {
            StackingLevel::BlockDescendants
        };

        let mut stacking_context = StackingContext::new(level, id as u32);
        if style.opacity < 1.0 {
            stacking_context = stacking_context.with_opacity(style.opacity);
        }
        if let Some(clip) = style.overflow_clip {
            stacking_context = stacking_context.with_clip(clip);
        }

        self.nodes.insert(
            id,
            PaintNode {
                id,
                parent,
                children: Vec::new(),
                stacking_context,
            },
        );

        // Add to parent's children list
        if let Some(parent_id) = parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent_id) {
                parent_node.children.push(id);
            }
        }
    }

    /// Build the display list in correct paint order.
    #[must_use]
    pub fn build(self, root: NodeId) -> DisplayList {
        let paint_order = traverse_paint_tree(root, &self.nodes);
        let mut items = Vec::new();

        for entry in paint_order {
            if let Some(rect) = self.rects.get(&entry.node_id) {
                if let Some(style) = self.styles.get(&entry.node_id) {
                    // Paint background
                    if style.background_color[3] > 0.0 {
                        items.push(DisplayItem::Rect {
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            color: [
                                style.background_color[0],
                                style.background_color[1],
                                style.background_color[2],
                                style.background_color[3],
                            ],
                        });
                    }

                    // Paint text content
                    if let Some(LayoutNodeKind::InlineText { text }) =
                        self.kinds.get(&entry.node_id)
                    {
                        items.push(DisplayItem::Text {
                            x: rect.x,
                            y: rect.y + style.font_size, // baseline
                            text: text.clone(),
                            color: style.text_color,
                            font_size: style.font_size,
                            bounds: Some((
                                rect.x as i32,
                                rect.y as i32,
                                (rect.x + rect.width) as i32,
                                (rect.y + rect.height) as i32,
                            )),
                        });
                    }
                }
            }
        }

        DisplayList::from_items(items)
    }
}

impl Default for DisplayListBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_display_list() {
        let mut builder = DisplayListBuilder::new();

        builder.add_node(
            0,
            None,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            },
            PaintStyle {
                background_color: [1.0, 1.0, 1.0, 1.0],
                ..Default::default()
            },
            LayoutNodeKind::Block,
        );

        builder.add_node(
            1,
            Some(0),
            LayoutRect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 50.0,
            },
            PaintStyle {
                background_color: [0.0, 0.5, 1.0, 1.0],
                ..Default::default()
            },
            LayoutNodeKind::Block,
        );

        let display_list = builder.build(0);
        assert_eq!(display_list.items.len(), 2);
    }
}
