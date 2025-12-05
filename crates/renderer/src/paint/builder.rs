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
    /// Font weight (100-900, default 400 = normal, 700 = bold).
    pub font_weight: u16,
    /// Font family (e.g., "Courier New", "monospace").
    pub font_family: Option<String>,
    /// Opacity (0.0 = transparent, 1.0 = opaque).
    pub opacity: f32,
    /// Z-index for positioned elements.
    pub z_index: Option<i32>,
    /// Whether element is positioned.
    pub is_positioned: bool,
    /// Overflow clipping bounds.
    pub overflow_clip: Option<super::stacking::ClipRect>,
}

impl Default for PaintStyle {
    fn default() -> Self {
        Self {
            background_color: [0.0, 0.0, 0.0, 0.0],
            text_color: [0.0, 0.0, 0.0],
            font_size: 16.0,
            font_weight: 400,
            font_family: None,
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
    InlineText {
        /// Text content.
        text: String,
    },
    /// Positioned element.
    Positioned,
}

/// Data for adding a node to the paint tree.
#[derive(Debug, Clone)]
pub struct PaintNodeData {
    /// Node identifier.
    pub id: NodeId,
    /// Parent node.
    pub parent: Option<NodeId>,
    /// Layout rectangle.
    pub rect: LayoutRect,
    /// Paint style.
    pub style: PaintStyle,
    /// Node kind.
    pub kind: LayoutNodeKind,
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
    pub fn add_node(&mut self, node: PaintNodeData) {
        let id = node.id;
        let parent = node.parent;
        self.rects.insert(id, node.rect);
        self.styles.insert(id, node.style.clone());
        self.kinds.insert(id, node.kind);

        // Determine stacking level
        let level = if node.style.is_positioned {
            node.style.z_index.map_or(
                StackingLevel::PositionedZeroOrAuto,
                StackingLevel::from_z_index,
            )
        } else {
            StackingLevel::BlockDescendants
        };

        let mut stacking_context = StackingContext::new(level, id as u32);
        if node.style.opacity < 1.0 {
            stacking_context = stacking_context.with_opacity(node.style.opacity);
        }
        if let Some(clip) = node.style.overflow_clip {
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
        if let Some(parent_id) = parent
            && let Some(parent_node) = self.nodes.get_mut(&parent_id)
        {
            parent_node.children.push(id);
        }
    }

    /// Build the display list in correct paint order.
    #[must_use]
    pub fn build(self, root: NodeId) -> DisplayList {
        let paint_order = traverse_paint_tree(root, &self.nodes);
        let mut items = Vec::new();

        for entry in paint_order {
            self.paint_entry(&entry, &mut items);
        }

        DisplayList::from_items(items)
    }

    /// Paint a single entry in the paint order.
    fn paint_entry(&self, entry: &super::traversal::PaintOrder, items: &mut Vec<DisplayItem>) {
        let Some(rect) = self.rects.get(&entry.node_id) else {
            return;
        };
        let Some(style) = self.styles.get(&entry.node_id) else {
            return;
        };

        // Paint background
        Self::paint_background(rect, style, items);

        // Paint text content
        self.paint_text_content(entry.node_id, rect, style, items);
    }

    /// Paint the background of a node.
    fn paint_background(rect: &LayoutRect, style: &PaintStyle, items: &mut Vec<DisplayItem>) {
        if style.background_color[3] > 0.0 {
            items.push(DisplayItem::Rect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                color: style.background_color,
            });
        }
    }

    /// Paint text content if this is a text node.
    fn paint_text_content(
        &self,
        node_id: NodeId,
        rect: &LayoutRect,
        style: &PaintStyle,
        items: &mut Vec<DisplayItem>,
    ) {
        if let Some(LayoutNodeKind::InlineText { text }) = self.kinds.get(&node_id) {
            // Calculate line height (matches css_text::measurement logic)
            let line_height = match style.font_size.round() as i32 {
                14 => 17.0,
                16 => 18.0,
                18 => 22.0,
                24 => 28.0,
                _ => (style.font_size * 1.125).round(),
            };

            items.push(DisplayItem::Text {
                x: rect.x,
                y: rect.y + style.font_size, // baseline
                text: text.clone(),
                color: style.text_color,
                font_size: style.font_size,
                font_weight: style.font_weight,
                font_family: style.font_family.clone(),
                line_height,
                bounds: Some((
                    rect.x.round() as i32,
                    rect.y.round() as i32,
                    (rect.x + rect.width).round() as i32,
                    (rect.y + rect.height).round() as i32,
                )),
            });
        }
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

    /// Test basic display list building with parent and child nodes.
    ///
    /// # Panics
    /// Panics if the display list does not contain exactly 2 items.
    #[test]
    fn basic_display_list() {
        let mut builder = DisplayListBuilder::new();

        builder.add_node(PaintNodeData {
            id: 0,
            parent: None,
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            },
            style: PaintStyle {
                background_color: [1.0, 1.0, 1.0, 1.0],
                ..Default::default()
            },
            kind: LayoutNodeKind::Block,
        });

        builder.add_node(PaintNodeData {
            id: 1,
            parent: Some(0),
            rect: LayoutRect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 50.0,
            },
            style: PaintStyle {
                background_color: [0.0, 0.5, 1.0, 1.0],
                ..Default::default()
            },
            kind: LayoutNodeKind::Block,
        });

        let display_list = builder.build(0);
        assert_eq!(display_list.items.len(), 2);
    }
}
