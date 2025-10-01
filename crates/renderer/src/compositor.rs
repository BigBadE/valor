//! Opacity compositor for handling stacking contexts and multi-pass rendering.
//!
//! This module provides high-level orchestration for opacity rendering, separating
//! the "what to render" logic from the low-level GPU operations in wgpu_backend.

use crate::display_list::{DisplayItem, DisplayList, StackingContextBoundary};

/// A rectangular region in device-independent pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Expand this rect to include another rect.
    pub fn union(&mut self, other: Self) {
        let x2 = (self.x + self.width).max(other.x + other.width);
        let y2 = (self.y + self.height).max(other.y + other.height);
        self.x = self.x.min(other.x);
        self.y = self.y.min(other.y);
        self.width = x2 - self.x;
        self.height = y2 - self.y;
    }
}

/// An opacity group that needs to be rendered offscreen and composited.
#[derive(Debug, Clone)]
pub struct OpacityGroup {
    /// Index of BeginStackingContext in the display list.
    pub start_index: usize,
    /// Index of EndStackingContext in the display list.
    pub end_index: usize,
    /// Opacity value (0.0 to 1.0).
    pub alpha: f32,
    /// Bounding rectangle for the group.
    pub bounds: Rect,
    /// Display items within this group (excluding Begin/End markers).
    pub items: Vec<DisplayItem>,
}

/// High-level compositor for managing opacity groups and stacking contexts.
pub struct OpacityCompositor {
    groups: Vec<OpacityGroup>,
}

impl OpacityCompositor {
    /// Collect all opacity groups from a display list.
    pub fn collect_from_display_list(dl: &DisplayList) -> Self {
        let mut groups = Vec::new();
        let items = &dl.items;

        let mut i = 0;
        while i < items.len() {
            if Self::is_opacity_stacking_context(&items[i]) {
                // Find the matching EndStackingContext
                let end_index = Self::find_stacking_context_end(items, i + 1);

                // Extract items within the group
                let group_items: Vec<DisplayItem> = items[i + 1..end_index].to_vec();

                // Compute bounds for the group
                let bounds =
                    Self::compute_bounds(&group_items).unwrap_or(Rect::new(0.0, 0.0, 1.0, 1.0));

                // Extract alpha value
                let alpha = if let DisplayItem::BeginStackingContext {
                    boundary: StackingContextBoundary::Opacity { alpha },
                } = &items[i]
                {
                    *alpha
                } else {
                    1.0
                };

                groups.push(OpacityGroup {
                    start_index: i,
                    end_index,
                    alpha,
                    bounds,
                    items: group_items,
                });

                i = end_index + 1;
                continue;
            }
            i += 1;
        }

        Self { groups }
    }

    /// Check if a display item is an opacity stacking context.
    fn is_opacity_stacking_context(item: &DisplayItem) -> bool {
        matches!(
            item,
            DisplayItem::BeginStackingContext {
                boundary: StackingContextBoundary::Opacity { alpha }
            } if *alpha < 1.0
        )
    }

    /// Check if any opacity groups need offscreen rendering.
    pub const fn needs_offscreen_rendering(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Get all opacity groups.
    pub fn groups(&self) -> &[OpacityGroup] {
        &self.groups
    }

    /// Compute bounding box for a slice of display items.
    /// Returns (x, y, width, height) or None if no items have bounds.
    pub fn compute_items_bounds(items: &[DisplayItem]) -> Option<(f32, f32, f32, f32)> {
        Self::compute_bounds(items).map(|r| (r.x, r.y, r.width, r.height))
    }

    /// Find the matching EndStackingContext for a BeginStackingContext.
    pub fn find_stacking_context_end(items: &[DisplayItem], start: usize) -> usize {
        let mut depth = 1i32;
        for (idx, item) in items.iter().enumerate().skip(start) {
            match item {
                DisplayItem::BeginStackingContext { .. } => depth += 1i32,
                DisplayItem::EndStackingContext => {
                    depth -= 1i32;
                    if depth == 0i32 {
                        return idx;
                    }
                }
                _ => {}
            }
        }
        items.len().saturating_sub(1)
    }

    /// Compute the bounding rectangle for a set of display items.
    fn compute_bounds(items: &[DisplayItem]) -> Option<Rect> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut found_any = false;

        for item in items {
            match item {
                DisplayItem::Rect {
                    x,
                    y,
                    width,
                    height,
                    ..
                } => {
                    min_x = min_x.min(*x);
                    min_y = min_y.min(*y);
                    max_x = max_x.max(x + width);
                    max_y = max_y.max(y + height);
                    found_any = true;
                }
                DisplayItem::Text { x, y, bounds, .. } => {
                    if let Some((left, top, right, bottom)) = bounds {
                        min_x = min_x.min(*left as f32);
                        min_y = min_y.min(*top as f32);
                        max_x = max_x.max(*right as f32);
                        max_y = max_y.max(*bottom as f32);
                    } else {
                        // Fallback: use text position with estimated size
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y);
                        max_x = max_x.max(x + 100.0);
                        max_y = max_y.max(y + 20.0);
                    }
                    found_any = true;
                }
                DisplayItem::BeginStackingContext { .. }
                | DisplayItem::EndStackingContext
                | DisplayItem::BeginClip { .. }
                | DisplayItem::EndClip => {
                    // These don't contribute to bounds
                }
            }
        }

        (found_any && min_x.is_finite() && min_y.is_finite()).then(|| {
            Rect::new(
                min_x,
                min_y,
                (max_x - min_x).max(1.0),
                (max_y - min_y).max(1.0),
            )
        })
    }

    /// Get ranges to exclude from main rendering (the opacity group regions).
    pub fn exclude_ranges(&self) -> Vec<(usize, usize)> {
        self.groups
            .iter()
            .map(|g| (g.start_index, g.end_index))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_bounds_empty() {
        let items = vec![];
        assert!(OpacityCompositor::compute_bounds(&items).is_none());
    }

    #[test]
    #[allow(
        clippy::unwrap_used,
        reason = "Test code may use unwrap for simplicity"
    )]
    fn compute_bounds_single_rect() {
        let items = vec![DisplayItem::Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
            color: [1.0, 0.0, 0.0, 1.0],
        }];
        let bounds = OpacityCompositor::compute_bounds(&items).unwrap();
        assert!((bounds.x - 10.0).abs() < f32::EPSILON);
        assert!((bounds.y - 20.0).abs() < f32::EPSILON);
        assert!((bounds.width - 100.0).abs() < f32::EPSILON);
        assert!((bounds.height - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn find_stacking_context_end_works() {
        let items = vec![
            DisplayItem::BeginStackingContext {
                boundary: StackingContextBoundary::Opacity { alpha: 0.5 },
            },
            DisplayItem::Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
                color: [1.0, 0.0, 0.0, 1.0],
            },
            DisplayItem::EndStackingContext,
        ];
        let end = OpacityCompositor::find_stacking_context_end(&items, 1);
        assert_eq!(end, 2);
    }

    #[test]
    fn collect_opacity_groups() {
        let mut dl = DisplayList::new();
        dl.items.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
            color: [1.0, 1.0, 1.0, 1.0],
        });
        dl.items.push(DisplayItem::BeginStackingContext {
            boundary: StackingContextBoundary::Opacity { alpha: 0.5 },
        });
        dl.items.push(DisplayItem::Rect {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 100.0,
            color: [1.0, 0.0, 0.0, 1.0],
        });
        dl.items.push(DisplayItem::EndStackingContext);

        let compositor = OpacityCompositor::collect_from_display_list(&dl);
        assert_eq!(compositor.groups().len(), 1);
        assert!((compositor.groups()[0].alpha - 0.5).abs() < f32::EPSILON);
        assert!(compositor.needs_offscreen_rendering());
    }
}
