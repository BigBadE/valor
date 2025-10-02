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
    pub const fn intersects(self, other: Self) -> bool {
        let self_right = self.x + self.width as i32;
        let self_bottom = self.y + self.height as i32;
        let other_right = other.x + other.width as i32;
        let other_bottom = other.y + other.height as i32;

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
        let right = (self.x + self.width as i32).max(other.x + other.width as i32);
        let bottom = (self.y + self.height as i32).max(other.y + other.height as i32);
        Self {
            x,
            y,
            width: (right - x) as u32,
            height: (bottom - y) as u32,
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
    pub fn new(framebuffer_width: u32, framebuffer_height: u32) -> Self {
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
        let x = rect.x.max(0);
        let y = rect.y.max(0);
        let right = (rect.x + rect.width as i32).min(self.framebuffer_width as i32);
        let bottom = (rect.y + rect.height as i32).min(self.framebuffer_height as i32);

        if right <= x || bottom <= y {
            return; // Empty rect
        }

        let clamped = DamageRect::new(x, y, (right - x) as u32, (bottom - y) as u32);

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

        // Simple greedy merging: find the two most overlapping rects and merge them
        let mut best_i = 0;
        let mut best_j = 1;
        let mut best_overlap = 0;

        for i in 0..self.damaged_rects.len() {
            for j in (i + 1)..self.damaged_rects.len() {
                if self.damaged_rects[i].intersects(self.damaged_rects[j]) {
                    let union = self.damaged_rects[i].union(self.damaged_rects[j]);
                    let overlap = self.damaged_rects[i].area() + self.damaged_rects[j].area()
                        - union.area();
                    if overlap > best_overlap {
                        best_overlap = overlap;
                        best_i = i;
                        best_j = j;
                    }
                }
            }
        }

        if best_overlap > 0 {
            let merged = self.damaged_rects[best_i].union(self.damaged_rects[best_j]);
            self.damaged_rects.remove(best_j);
            self.damaged_rects[best_i] = merged;
        } else {
            // No overlapping rects, merge the two smallest by area
            let mut areas: Vec<(usize, u32)> = self
                .damaged_rects
                .iter()
                .enumerate()
                .map(|(i, r)| (i, r.area()))
                .collect();
            areas.sort_by_key(|a| a.1);
            let i = areas[0].0;
            let j = areas[1].0;
            let merged = self.damaged_rects[i].union(self.damaged_rects[j]);
            let (min_idx, max_idx) = if i < j { (i, j) } else { (j, i) };
            self.damaged_rects.remove(max_idx);
            self.damaged_rects[min_idx] = merged;
        }
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

    #[test]
    fn damage_all() {
        let mut tracker = DamageTracker::new(800, 600);
        tracker.damage_all();
        assert!(tracker.is_fully_damaged());
    }

    #[test]
    fn damage_rect_merging() {
        let mut tracker = DamageTracker::new(800, 600);
        tracker.damage_rect(DamageRect::new(0, 0, 100, 100));
        tracker.damage_rect(DamageRect::new(50, 50, 100, 100));
        // Should merge into one rect
        assert_eq!(tracker.get_damaged_rects().len(), 1);
        let rect = tracker.get_damaged_rects()[0];
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
        assert_eq!(rect.width, 150);
        assert_eq!(rect.height, 150);
    }

    #[test]
    fn clear_damage() {
        let mut tracker = DamageTracker::new(800, 600);
        tracker.damage_rect(DamageRect::new(0, 0, 100, 100));
        tracker.clear();
        assert_eq!(tracker.get_damaged_rects().len(), 0);
    }
}
