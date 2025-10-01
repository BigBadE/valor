//! Opacity compositor for handling stacking contexts and multi-pass rendering.
//!
//! This module provides high-level orchestration for opacity rendering, separating
//! the "what to render" logic from the low-level GPU operations in `wgpu_backend`.

use crate::display_list::{DisplayItem, DisplayList, StackingContextBoundary};

/// Bounding box represented as (x, y, width, height).
pub type BoundingBox = (f32, f32, f32, f32);

/// A rectangular region in device-independent pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// Create a new rectangle.
    #[inline]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Expand this rectangle to include another rectangle.
    #[inline]
    pub fn union(&mut self, other: Self) {
        let max_x = (self.x + self.width).max(other.x + other.width);
        let max_y = (self.y + self.height).max(other.y + other.height);
        self.x = self.x.min(other.x);
        self.y = self.y.min(other.y);
        self.width = max_x - self.x;
        self.height = max_y - self.y;
    }
}

/// An opacity group that needs to be rendered offscreen and composited.
#[derive(Debug, Clone)]
pub struct OpacityGroup {
    /// Index of `BeginStackingContext` in the display list.
    pub start_index: usize,
    /// Index of `EndStackingContext` in the display list.
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
    /// List of collected opacity groups.
    groups: Vec<OpacityGroup>,
}

impl OpacityCompositor {
    /// Collect opacity groups from a display list.
    #[inline]
    pub fn collect_from_display_list(display_list: &DisplayList) -> Self {
        let mut groups = Vec::new();
        let items = &display_list.items;
        let mut index = 0;
        while index < items.len() {
            if Self::is_opacity_stacking_context(&items[index]) {
                // Find the matching EndStackingContext
                let end = Self::find_stacking_context_end(items, index + 1);

                // Extract items within the group
                let group_items = &items[index + 1..end];

                // Compute bounds for the group
                let bounds =
                    Self::compute_bounds(group_items).unwrap_or(Rect::new(0.0, 0.0, 1.0, 1.0));

                // Extract alpha value
                let alpha = if let DisplayItem::BeginStackingContext {
                    boundary: StackingContextBoundary::Opacity { alpha },
                } = &items[index]
                {
                    *alpha
                } else {
                    1.0
                };

                groups.push(OpacityGroup {
                    start_index: index,
                    end_index: end,
                    alpha,
                    bounds,
                    items: group_items.to_vec(),
                });

                index = end + 1;
                continue;
            }
            index += 1;
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
    #[inline]
    pub const fn needs_offscreen_rendering(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Get all opacity groups.
    #[inline]
    pub fn groups(&self) -> &[OpacityGroup] {
        &self.groups
    }

    /// Compute bounding box for a slice of display items.
    /// Returns (x, y, width, height) or None if no items have bounds.
    #[inline]
    pub fn compute_items_bounds(items: &[DisplayItem]) -> Option<BoundingBox> {
        Self::compute_bounds(items).map(|rect| (rect.x, rect.y, rect.width, rect.height))
    }

    /// Find the matching `EndStackingContext` for a `BeginStackingContext`.
    #[inline]
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

    /// Get the exclude ranges for opacity groups.
    #[inline]
    pub fn exclude_ranges(&self) -> Vec<(usize, usize)> {
        self.groups
            .iter()
            .map(|group| (group.start_index, group.end_index))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that `compute_bounds` returns None for empty items.
    ///
    /// # Panics
    /// Panics if the test assertions fail.
    #[test]
    fn compute_bounds_empty() {
        let items = vec![];
        assert!(OpacityCompositor::compute_bounds(&items).is_none());
    }

    /// Test that `compute_bounds` correctly calculates bounds for a single rect.
    ///
    /// # Panics
    /// Panics if the test assertions fail.
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

    /// Test that `find_stacking_context_end` correctly finds matching end markers.
    ///
    /// # Panics
    /// Panics if the test assertions fail.
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

    /// Test that opacity groups are correctly collected from a display list.
    ///
    /// # Panics
    /// Panics if the test assertions fail.
    #[test]
    fn collect_opacity_groups() {
        let mut display_list = DisplayList::new();
        display_list.items.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
            color: [1.0, 1.0, 1.0, 1.0],
        });
        display_list.items.push(DisplayItem::BeginStackingContext {
            boundary: StackingContextBoundary::Opacity { alpha: 0.5 },
        });
        display_list.items.push(DisplayItem::Rect {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 100.0,
            color: [1.0, 0.0, 0.0, 1.0],
        });
        display_list.items.push(DisplayItem::EndStackingContext);

        let compositor = OpacityCompositor::collect_from_display_list(&display_list);
        assert_eq!(compositor.groups().len(), 1);
        assert!((compositor.groups()[0].alpha - 0.5).abs() < f32::EPSILON);
        assert!(compositor.needs_offscreen_rendering());
    }
}
