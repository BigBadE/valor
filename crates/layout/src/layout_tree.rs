//! Layout tree structure for storing computed layout results.
//!
//! The layout tree is the output of the layout computation phase. It contains
//! the final positions and dimensions of all boxes that will be rendered.
//!
//! Unlike the DOM tree (which represents the document structure) and the box tree
//! (which represents CSS boxes), the layout tree represents the final geometric
//! arrangement of elements on the page.

use crate::Subpixels;
use rewrite_core::NodeId;

/// A positioned box in the layout tree.
///
/// This represents the final computed layout for a DOM node after all CSS
/// layout algorithms have been applied.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    /// The DOM node this box represents.
    pub node: NodeId,

    /// The box's position and dimensions in the coordinate space of its
    /// containing block.
    pub content_rect: Rect,

    /// Padding edges (inside border, outside content).
    pub padding: EdgeSizes,

    /// Border widths.
    pub border: EdgeSizes,

    /// Margin (outside border). May be negative.
    pub margin: EdgeSizes,

    /// Children boxes. For block containers, these are block-level children.
    /// For inline containers, these are line boxes.
    pub children: Vec<LayoutBox>,

    /// Type of box.
    pub box_type: BoxType,
}

/// Rectangle representing position and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    /// X coordinate (inline offset from containing block's content edge).
    pub x: Subpixels,
    /// Y coordinate (block offset from containing block's content edge).
    pub y: Subpixels,
    /// Width (inline size).
    pub width: Subpixels,
    /// Height (block size).
    pub height: Subpixels,
}

impl Rect {
    /// Create a new rectangle.
    pub fn new(x: Subpixels, y: Subpixels, width: Subpixels, height: Subpixels) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create a rectangle at the origin with given size.
    pub fn from_size(width: Subpixels, height: Subpixels) -> Self {
        Self::new(0, 0, width, height)
    }

    /// Get the right edge (x + width).
    pub fn right(&self) -> Subpixels {
        self.x + self.width
    }

    /// Get the bottom edge (y + height).
    pub fn bottom(&self) -> Subpixels {
        self.y + self.height
    }

    /// Check if this rectangle contains a point.
    pub fn contains_point(&self, x: Subpixels, y: Subpixels) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// Check if this rectangle intersects another rectangle.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

/// Edge sizes (top, right, bottom, left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EdgeSizes {
    pub top: Subpixels,
    pub right: Subpixels,
    pub bottom: Subpixels,
    pub left: Subpixels,
}

impl EdgeSizes {
    /// Create edge sizes with all edges set to the same value.
    pub fn uniform(size: Subpixels) -> Self {
        Self {
            top: size,
            right: size,
            bottom: size,
            left: size,
        }
    }

    /// Create edge sizes from individual values.
    pub fn new(top: Subpixels, right: Subpixels, bottom: Subpixels, left: Subpixels) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Get the sum of horizontal edges (left + right).
    pub fn horizontal(&self) -> Subpixels {
        self.left + self.right
    }

    /// Get the sum of vertical edges (top + bottom).
    pub fn vertical(&self) -> Subpixels {
        self.top + self.bottom
    }
}

/// Type of layout box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxType {
    /// Block-level box (stacks vertically).
    Block,
    /// Inline-level box (flows horizontally).
    Inline,
    /// Inline-block box (inline flow, but has block dimensions).
    InlineBlock,
    /// Line box (contains inline content).
    Line,
    /// Text run within a line box.
    Text,
    /// Flex container.
    Flex,
    /// Flex item.
    FlexItem,
    /// Grid container.
    Grid,
    /// Grid item.
    GridItem,
    /// Table container.
    Table,
    /// Table row.
    TableRow,
    /// Table cell.
    TableCell,
    /// Absolutely positioned box.
    Absolute,
    /// Fixed positioned box.
    Fixed,
    /// Floated box.
    Float,
}

impl LayoutBox {
    /// Create a new layout box.
    pub fn new(node: NodeId, box_type: BoxType) -> Self {
        Self {
            node,
            content_rect: Rect::default(),
            padding: EdgeSizes::default(),
            border: EdgeSizes::default(),
            margin: EdgeSizes::default(),
            children: Vec::new(),
            box_type,
        }
    }

    /// Get the padding box rectangle (content + padding).
    pub fn padding_rect(&self) -> Rect {
        Rect::new(
            self.content_rect.x - self.padding.left,
            self.content_rect.y - self.padding.top,
            self.content_rect.width + self.padding.horizontal(),
            self.content_rect.height + self.padding.vertical(),
        )
    }

    /// Get the border box rectangle (content + padding + border).
    pub fn border_rect(&self) -> Rect {
        Rect::new(
            self.content_rect.x - self.padding.left - self.border.left,
            self.content_rect.y - self.padding.top - self.border.top,
            self.content_rect.width + self.padding.horizontal() + self.border.horizontal(),
            self.content_rect.height + self.padding.vertical() + self.border.vertical(),
        )
    }

    /// Get the margin box rectangle (content + padding + border + margin).
    pub fn margin_rect(&self) -> Rect {
        let border_rect = self.border_rect();
        Rect::new(
            border_rect.x - self.margin.left,
            border_rect.y - self.margin.top,
            border_rect.width + self.margin.horizontal(),
            border_rect.height + self.margin.vertical(),
        )
    }

    /// Add a child box.
    pub fn add_child(&mut self, child: LayoutBox) {
        self.children.push(child);
    }

    /// Get total inline size (width + padding + border).
    pub fn border_box_width(&self) -> Subpixels {
        self.content_rect.width + self.padding.horizontal() + self.border.horizontal()
    }

    /// Get total block size (height + padding + border).
    pub fn border_box_height(&self) -> Subpixels {
        self.content_rect.height + self.padding.vertical() + self.border.vertical()
    }
}

/// Layout tree builder that constructs the layout tree from the DOM.
pub struct LayoutTreeBuilder {
    /// Root of the layout tree.
    root: Option<LayoutBox>,
}

impl LayoutTreeBuilder {
    /// Create a new layout tree builder.
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Set the root box.
    pub fn set_root(&mut self, root: LayoutBox) {
        self.root = Some(root);
    }

    /// Build and return the layout tree.
    pub fn build(self) -> Option<LayoutBox> {
        self.root
    }
}

impl Default for LayoutTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}
