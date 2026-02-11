//! Damage tracking for partial redraws.
//!
//! Tracks which regions of the screen need to be redrawn to optimize
//! rendering performance by skipping unchanged areas.

/// A rectangular region that needs to be redrawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageRect {
    /// X coordinate in pixels.
    pub x: i32,
    /// Y coordinate in pixels.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl DamageRect {
    /// Create a new damage rectangle.
    #[must_use]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if this rectangle intersects another.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        let self_right = self
            .x
            .saturating_add(i32::try_from(self.width).unwrap_or(i32::MAX));
        let self_bottom = self
            .y
            .saturating_add(i32::try_from(self.height).unwrap_or(i32::MAX));
        let other_right = other
            .x
            .saturating_add(i32::try_from(other.width).unwrap_or(i32::MAX));
        let other_bottom = other
            .y
            .saturating_add(i32::try_from(other.height).unwrap_or(i32::MAX));

        self.x < other_right
            && self_right > other.x
            && self.y < other_bottom
            && self_bottom > other.y
    }

    /// Compute the union of two rectangles.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let self_right = self
            .x
            .saturating_add(i32::try_from(self.width).unwrap_or(i32::MAX));
        let other_right = other
            .x
            .saturating_add(i32::try_from(other.width).unwrap_or(i32::MAX));
        let self_bottom = self
            .y
            .saturating_add(i32::try_from(self.height).unwrap_or(i32::MAX));
        let other_bottom = other
            .y
            .saturating_add(i32::try_from(other.height).unwrap_or(i32::MAX));
        let right = self_right.max(other_right);
        let bottom = self_bottom.max(other_bottom);
        Self {
            x,
            y,
            width: u32::try_from(right.saturating_sub(x)).unwrap_or(0),
            height: u32::try_from(bottom.saturating_sub(y)).unwrap_or(0),
        }
    }

    /// Get the area of this rectangle in pixels.
    #[must_use]
    pub const fn area(self) -> u32 {
        self.width * self.height
    }
}

/// Tracks damaged regions for incremental rendering.
pub struct DamageTracker {
    /// Set of damaged rectangles.
    damaged_rects: Vec<DamageRect>,
    /// Framebuffer dimensions.
    framebuffer_width: u32,
    /// Framebuffer height.
    framebuffer_height: u32,
    /// Maximum number of damage rects before merging.
    max_rects: usize,
}

impl DamageTracker {
    /// Create a new damage tracker.
    #[must_use]
    pub const fn new(framebuffer_width: u32, framebuffer_height: u32) -> Self {
        Self {
            damaged_rects: Vec::new(),
            framebuffer_width,
            framebuffer_height,
            max_rects: 10,
        }
    }

    /// Mark the entire framebuffer as damaged.
    pub fn damage_all(&mut self) {
        self.damaged_rects.clear();
        self.damaged_rects.push(DamageRect::new(
            0,
            0,
            self.framebuffer_width,
            self.framebuffer_height,
        ));
    }

    /// Add a damaged rectangle.
    pub fn damage_rect(&mut self, rect: DamageRect) {
        // Clamp to framebuffer bounds
        let x = rect.x.max(0i32);
        let y = rect.y.max(0i32);
        let rect_right = rect
            .x
            .saturating_add(i32::try_from(rect.width).unwrap_or(i32::MAX));
        let rect_bottom = rect
            .y
            .saturating_add(i32::try_from(rect.height).unwrap_or(i32::MAX));
        let fb_width = i32::try_from(self.framebuffer_width).unwrap_or(i32::MAX);
        let fb_height = i32::try_from(self.framebuffer_height).unwrap_or(i32::MAX);
        let right = rect_right.min(fb_width);
        let bottom = rect_bottom.min(fb_height);

        if right <= x || bottom <= y {
            return; // Empty rect
        }

        let clamped = DamageRect::new(
            x,
            y,
            u32::try_from(right - x).unwrap_or(0),
            u32::try_from(bottom - y).unwrap_or(0),
        );

        // Try to merge with existing rects
        let mut merged = false;
        for existing in &mut self.damaged_rects {
            if existing.intersects(clamped) {
                *existing = existing.union(clamped);
                merged = true;
                break;
            }
        }

        if !merged {
            self.damaged_rects.push(clamped);
        }

        // If we have too many rects, merge them
        if self.damaged_rects.len() > self.max_rects {
            self.merge_rects();
        }
    }

    /// Get the damaged rectangles.
    #[must_use]
    pub fn get_damaged_rects(&self) -> &[DamageRect] {
        &self.damaged_rects
    }

    /// Check if the entire framebuffer is damaged.
    #[must_use]
    pub fn is_fully_damaged(&self) -> bool {
        if self.damaged_rects.len() != 1 {
            return false;
        }
        let rect = self.damaged_rects[0];
        rect.x == 0
            && rect.y == 0
            && rect.width == self.framebuffer_width
            && rect.height == self.framebuffer_height
    }

    /// Clear all damage.
    pub fn clear(&mut self) {
        self.damaged_rects.clear();
    }

    /// Resize the framebuffer and damage all.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.framebuffer_width = width;
        self.framebuffer_height = height;
        self.damage_all();
    }

    /// Merge overlapping damage rects.
    fn merge_rects(&mut self) {
        if self.damaged_rects.len() <= 1 {
            return;
        }

        let (best_i, best_j) = self.find_best_merge_candidates();
        self.merge_pair(best_i, best_j);
    }

    /// Find the best pair of rects to merge (most overlap or smallest if no overlap).
    fn find_best_merge_candidates(&self) -> (usize, usize) {
        let mut best_i = 0;
        let mut best_j = 1;
        let mut best_overlap = 0;

        for first_idx in 0..self.damaged_rects.len() {
            for second_idx in (first_idx + 1)..self.damaged_rects.len() {
                let overlap = self.calculate_overlap(first_idx, second_idx);
                if overlap > best_overlap {
                    best_overlap = overlap;
                    best_i = first_idx;
                    best_j = second_idx;
                }
            }
        }

        if best_overlap > 0 {
            (best_i, best_j)
        } else {
            self.find_smallest_pair()
        }
    }

    /// Calculate overlap between two rects.
    fn calculate_overlap(&self, first_idx: usize, second_idx: usize) -> u32 {
        let first = &self.damaged_rects[first_idx];
        let second = &self.damaged_rects[second_idx];
        if !first.intersects(*second) {
            return 0;
        }
        let union = first.union(*second);
        first.area() + second.area() - union.area()
    }

    /// Find the two smallest rects when no overlaps exist.
    fn find_smallest_pair(&self) -> (usize, usize) {
        let mut areas: Vec<(usize, u32)> = self
            .damaged_rects
            .iter()
            .enumerate()
            .map(|(idx, rect)| (idx, rect.area()))
            .collect();
        areas.sort_by_key(|entry| entry.1);
        let first_idx = areas[0].0;
        let second_idx = areas[1].0;
        if first_idx < second_idx {
            (first_idx, second_idx)
        } else {
            (second_idx, first_idx)
        }
    }

    /// Merge a pair of rects at the given indices.
    fn merge_pair(&mut self, min_idx: usize, max_idx: usize) {
        let merged = self.damaged_rects[min_idx].union(self.damaged_rects[max_idx]);
        self.damaged_rects.remove(max_idx);
        self.damaged_rects[min_idx] = merged;
    }
}

impl Default for DamageTracker {
    fn default() -> Self {
        Self::new(800, 600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that damage tracking can mark the entire surface as damaged.
    ///
    /// # Panics
    /// Panics if the damage tracker does not report as fully damaged after calling `damage_all`.
    #[test]
    fn damage_all() {
        let mut tracker = DamageTracker::new(800, 600);
        tracker.damage_all();
        assert!(tracker.is_fully_damaged());
    }

    /// Test that overlapping damage rectangles are properly merged.
    ///
    /// # Panics
    /// Panics if the damaged rectangles are not properly merged or dimensions are incorrect.
    #[test]
    fn damage_rect_merging() {
        let mut tracker = DamageTracker::new(800, 600);
        tracker.damage_rect(DamageRect::new(0, 0, 100, 100));
        tracker.damage_rect(DamageRect::new(50, 50, 100, 100));
        // Should merge into one rect
        assert_eq!(tracker.get_damaged_rects().len(), 1);
        let rect = tracker.get_damaged_rects()[0];
        assert_eq!(rect.x, 0i32);
        assert_eq!(rect.y, 0i32);
        assert_eq!(rect.width, 150);
        assert_eq!(rect.height, 150);
    }

    /// Test that damage tracking can be cleared.
    ///
    /// # Panics
    /// Panics if damaged rectangles are not cleared after calling `clear`.
    #[test]
    fn clear_damage() {
        let mut tracker = DamageTracker::new(800, 600);
        tracker.damage_rect(DamageRect::new(0, 0, 100, 100));
        tracker.clear();
        assert_eq!(tracker.get_damaged_rects().len(), 0);
    }
}
