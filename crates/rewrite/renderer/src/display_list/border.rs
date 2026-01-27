//! Border and outline rendering types.

use rewrite_core::{Color, Edge, Keyword};

use super::BorderRadius;

/// Border with individual edge styles.
#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    edges: [BorderEdge; 4], // Indexed by Edge enum
    /// Border radius for rounded corners.
    pub radius: BorderRadius,
}

impl Border {
    pub fn new(top: BorderEdge, right: BorderEdge, bottom: BorderEdge, left: BorderEdge) -> Self {
        let mut edges = [BorderEdge::none(); 4];
        edges[Edge::Top as usize] = top;
        edges[Edge::Right as usize] = right;
        edges[Edge::Bottom as usize] = bottom;
        edges[Edge::Left as usize] = left;
        Self {
            edges,
            radius: BorderRadius::zero(),
        }
    }

    pub fn uniform(edge: BorderEdge) -> Self {
        Self {
            edges: [edge; 4],
            radius: BorderRadius::zero(),
        }
    }

    pub fn get(&self, edge: Edge) -> BorderEdge {
        self.edges[edge as usize]
    }

    pub fn set(&mut self, edge: Edge, border_edge: BorderEdge) {
        self.edges[edge as usize] = border_edge;
    }
}

/// Single border edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderEdge {
    pub width: f32,
    pub style: Keyword, // None, Hidden, Solid, Dashed, Dotted, Double, Groove, Ridge, Inset, Outset
    pub color: Color,
}

impl BorderEdge {
    pub const fn none() -> Self {
        Self {
            width: 0.0,
            style: Keyword::None,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        }
    }

    pub const fn new(width: f32, style: Keyword, color: Color) -> Self {
        Self {
            width,
            style,
            color,
        }
    }
}

/// Outline (drawn outside border).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    pub width: f32,
    pub style: Keyword, // None, Solid, Dashed, Dotted, Double, Groove, Ridge, Inset, Outset
    pub color: Color,
    pub offset: f32,
}
