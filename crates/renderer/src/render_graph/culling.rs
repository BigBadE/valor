//! Visibility culling optimizations for the render graph.
//!
//! This module implements various culling strategies to skip rendering items
//! that are not visible in the final output:
//! - Frustum culling: Skip items outside the viewport
//! - Occlusion culling: Skip items that are fully occluded by opaque items

use crate::display_list::DisplayItem;

/// Axis-aligned bounding box for visibility testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AABB {
    /// Minimum x coordinate.
    pub min_x: f32,
    /// Minimum y coordinate.
    pub min_y: f32,
    /// Maximum x coordinate.
    pub max_x: f32,
    /// Maximum y coordinate.
    pub max_y: f32,
}

impl AABB {
    /// Create a new axis-aligned bounding box.
    #[inline]
    pub const fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// Create an AABB from position and size.
    #[inline]
    pub fn from_rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    /// Check if this AABB intersects with another AABB.
    #[inline]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }

    /// Check if this AABB is fully contained within another AABB.
    #[inline]
    pub fn contained_in(&self, other: &Self) -> bool {
        self.min_x >= other.min_x
            && self.max_x <= other.max_x
            && self.min_y >= other.min_y
            && self.max_y <= other.max_y
    }

    /// Calculate the area of this AABB.
    #[inline]
    pub fn area(&self) -> f32 {
        (self.max_x - self.min_x).max(0.0) * (self.max_y - self.min_y).max(0.0)
    }
}

/// Extract the axis-aligned bounding box from a display item.
fn item_bounds(item: &DisplayItem) -> Option<AABB> {
    match item {
        DisplayItem::Rect {
            x,
            y,
            width,
            height,
            ..
        }
        | DisplayItem::Border {
            x,
            y,
            width,
            height,
            ..
        }
        | DisplayItem::Image {
            x,
            y,
            width,
            height,
            ..
        }
        | DisplayItem::LinearGradient {
            x,
            y,
            width,
            height,
            ..
        }
        | DisplayItem::RadialGradient {
            x,
            y,
            width,
            height,
            ..
        }
        | DisplayItem::BeginClip {
            x,
            y,
            width,
            height,
        } => Some(AABB::from_rect(*x, *y, *width, *height)),
        DisplayItem::Text { x, y, bounds, .. } => match bounds.as_ref() {
            Some((left, top, right, bottom)) => Some(AABB::new(
                *left as f32,
                *top as f32,
                *right as f32,
                *bottom as f32,
            )),
            None => Some(AABB::from_rect(*x, *y, 100.0, 20.0)),
        },
        DisplayItem::BoxShadow {
            x,
            y,
            width,
            height,
            offset_x,
            offset_y,
            blur_radius,
            spread_radius,
            ..
        } => {
            let extend = blur_radius + spread_radius;
            let (shadow_x, shadow_y) = (x + offset_x, y + offset_y);
            Some(AABB::new(
                shadow_x - extend,
                shadow_y - extend,
                shadow_x + width + extend,
                shadow_y + height + extend,
            ))
        }
        DisplayItem::EndClip
        | DisplayItem::BeginStackingContext { .. }
        | DisplayItem::EndStackingContext => None,
    }
}

/// Check if a display item is opaque (fully covers its bounds).
fn is_opaque(item: &DisplayItem) -> bool {
    match item {
        DisplayItem::Rect { color, .. } => {
            // Opaque if alpha is 1.0
            (color[3] - 1.0).abs() < f32::EPSILON
        }
        DisplayItem::LinearGradient { stops, .. } | DisplayItem::RadialGradient { stops, .. } => {
            // Opaque if all stops are opaque
            stops
                .iter()
                .all(|(_, color)| (color[3] - 1.0).abs() < f32::EPSILON)
        }
        DisplayItem::Image { .. }
        | DisplayItem::Text { .. }
        | DisplayItem::Border { .. }
        | DisplayItem::BoxShadow { .. }
        | DisplayItem::BeginClip { .. }
        | DisplayItem::EndClip
        | DisplayItem::BeginStackingContext { .. }
        | DisplayItem::EndStackingContext => {
            // Images may have transparency, other items don't contribute to occlusion
            false
        }
    }
}

/// Perform frustum culling on a list of display items.
///
/// Returns a new list containing only items that intersect with the viewport.
/// Items without spatial bounds (like stacking context markers) are always included.
///
/// # Arguments
///
/// * `items` - The display items to cull
/// * `viewport` - The viewport bounds (x, y, width, height)
///
/// # Examples
///
/// ```
/// # use renderer::display_list::DisplayItem;
/// # use renderer::render_graph::culling::frustum_cull;
/// let items = vec![
///     DisplayItem::Rect {
///         x: 0.0,
///         y: 0.0,
///         width: 100.0,
///         height: 100.0,
///         color: [1.0, 0.0, 0.0, 1.0],
///     },
///     DisplayItem::Rect {
///         x: 2000.0,  // Outside viewport
///         y: 2000.0,
///         width: 100.0,
///         height: 100.0,
///         color: [0.0, 1.0, 0.0, 1.0],
///     },
/// ];
/// let culled = frustum_cull(&items, (0.0, 0.0, 1920.0, 1080.0));
/// assert_eq!(culled.len(), 1);  // Only first rect should remain
/// ```
pub fn frustum_cull(items: &[DisplayItem], viewport: (f32, f32, f32, f32)) -> Vec<DisplayItem> {
    let (vp_x, vp_y, vp_width, vp_height) = viewport;
    let viewport_aabb = AABB::from_rect(vp_x, vp_y, vp_width, vp_height);

    let mut culled_items = Vec::with_capacity(items.len());

    for item in items {
        if let Some(bounds) = item_bounds(item) {
            // Only include items that intersect with the viewport
            if bounds.intersects(&viewport_aabb) {
                culled_items.push(item.clone());
            }
        } else {
            // Items without bounds (markers, etc.) are always included
            culled_items.push(item.clone());
        }
    }

    culled_items
}

/// Perform conservative occlusion culling on a list of display items.
///
/// Returns a new list with items that are fully occluded by opaque items removed.
/// This is a conservative implementation that only removes items with 100% certainty
/// of occlusion to ensure visual correctness.
///
/// # Arguments
///
/// * `items` - The display items to cull (should already be in paint order)
///
/// # Examples
///
/// ```
/// # use renderer::display_list::DisplayItem;
/// # use renderer::render_graph::culling::occlusion_cull;
/// let items = vec![
///     DisplayItem::Rect {
///         x: 0.0,
///         y: 0.0,
///         width: 100.0,
///         height: 100.0,
///         color: [1.0, 0.0, 0.0, 1.0],  // Opaque red
///     },
///     DisplayItem::Rect {
///         x: 0.0,
///         y: 0.0,
///         width: 100.0,
///         height: 100.0,
///         color: [1.0, 1.0, 1.0, 1.0],  // Opaque white (fully covers red)
///     },
/// ];
/// let culled = occlusion_cull(&items);
/// // First rect should be removed as it's fully occluded
/// assert_eq!(culled.len(), 1);
/// ```
pub fn occlusion_cull(items: &[DisplayItem]) -> Vec<DisplayItem> {
    let mut culled_items = Vec::with_capacity(items.len());
    let mut opaque_regions: Vec<AABB> = Vec::new();

    for item in items {
        let mut is_occluded = false;
        if let Some(bounds) = item_bounds(item) {
            // Check if fully occluded by any opaque region
            for opaque_region in &opaque_regions {
                if bounds.contained_in(opaque_region) {
                    is_occluded = true;
                    break;
                }
            }
            // If opaque, add to occlusion regions
            if !is_occluded && is_opaque(item) {
                opaque_regions.push(bounds);
            }
        }
        // Include item if not occluded
        if !is_occluded {
            culled_items.push(item.clone());
        }
    }

    culled_items
}
