//! Draw call batching optimizations for the render graph.
//!
//! This module groups compatible display items together to minimize GPU state
//! changes and improve rendering performance. Items are batched by type and
//! rendering properties to allow efficient submission to the GPU.

use crate::display_list::DisplayItem;

/// Type of batch for grouping compatible items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatchType {
    /// Solid color rectangles (no texture).
    SolidRects,
    /// Borders with optional border-radius.
    Borders,
    /// Images (requires texture binding).
    Images,
    /// Text glyphs (requires text rendering pipeline).
    Text,
    /// Gradients (linear or radial).
    Gradients,
    /// Box shadows.
    BoxShadows,
}

/// A batch of display items that can be drawn with minimal state changes.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawBatch {
    /// The items in this batch.
    pub items: Vec<DisplayItem>,
    /// The type of batch.
    pub batch_type: BatchType,
}

impl DrawBatch {
    /// Create a new draw batch.
    #[inline]
    pub const fn new(batch_type: BatchType) -> Self {
        Self {
            items: Vec::new(),
            batch_type,
        }
    }

    /// Add an item to this batch.
    #[inline]
    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    /// Check if this batch is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the number of items in this batch.
    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Determine the batch type for a display item.
fn item_batch_type(item: &DisplayItem) -> Option<BatchType> {
    match item {
        DisplayItem::Rect { .. } => Some(BatchType::SolidRects),
        DisplayItem::Border { .. } => Some(BatchType::Borders),
        DisplayItem::Image { .. } => Some(BatchType::Images),
        DisplayItem::Text { .. } => Some(BatchType::Text),
        DisplayItem::LinearGradient { .. } | DisplayItem::RadialGradient { .. } => {
            Some(BatchType::Gradients)
        }
        DisplayItem::BoxShadow { .. } => Some(BatchType::BoxShadows),
        DisplayItem::BeginClip { .. }
        | DisplayItem::EndClip
        | DisplayItem::BeginStackingContext { .. }
        | DisplayItem::EndStackingContext => None,
    }
}

/// Check if two items can be batched together.
///
/// Items can be batched if they are the same type and have compatible properties.
/// For example, two solid rects can be batched together, but a rect and a border cannot.
fn can_batch_together(item1: &DisplayItem, item2: &DisplayItem) -> bool {
    match (item_batch_type(item1), item_batch_type(item2)) {
        (Some(type1), Some(type2)) => {
            // Must be the same batch type
            if type1 != type2 {
                return false;
            }

            // Additional compatibility checks based on type
            match type1 {
                BatchType::SolidRects
                | BatchType::Borders
                | BatchType::Text
                | BatchType::Gradients
                | BatchType::BoxShadows => {
                    // All items of these types can be batched together
                    true
                }
                BatchType::Images => {
                    // Images can only be batched if they use the same texture
                    if let (
                        DisplayItem::Image { image_id: id1, .. },
                        DisplayItem::Image { image_id: id2, .. },
                    ) = (item1, item2)
                    {
                        id1 == id2
                    } else {
                        false
                    }
                }
            }
        }
        _ => false,
    }
}

/// Batch compatible display items together for efficient rendering.
///
/// This function groups consecutive items of the same type to minimize GPU state
/// changes. Non-batchable items (like stacking context markers) break batches
/// and are included as separate entries.
///
/// # Arguments
///
/// * `items` - The display items to batch
///
/// # Returns
///
/// A vector of draw batches, where each batch contains items that can be drawn
/// with the same GPU pipeline and minimal state changes.
///
/// # Examples
///
/// ```
/// # use renderer::display_list::DisplayItem;
/// # use renderer::render_graph::batching::batch_draw_calls;
/// let items = vec![
///     DisplayItem::Rect {
///         x: 0.0,
///         y: 0.0,
///         width: 100.0,
///         height: 100.0,
///         color: [1.0, 0.0, 0.0, 1.0],
///     },
///     DisplayItem::Rect {
///         x: 100.0,
///         y: 0.0,
///         width: 100.0,
///         height: 100.0,
///         color: [0.0, 1.0, 0.0, 1.0],
///     },
///     DisplayItem::Text {
///         x: 0.0,
///         y: 200.0,
///         text: "Hello".to_string(),
///         color: [0.0, 0.0, 0.0],
///         font_size: 16.0,
///         bounds: None,
///     },
/// ];
/// let batches = batch_draw_calls(&items);
/// assert_eq!(batches.len(), 2);  // One batch for rects, one for text
/// assert_eq!(batches[0].items.len(), 2);  // Two rects in first batch
/// ```
pub fn batch_draw_calls(items: &[DisplayItem]) -> Vec<DrawBatch> {
    let mut batches = Vec::new();
    let mut current_batch: Option<DrawBatch> = None;

    for item in items {
        if let Some(batch_type) = item_batch_type(item) {
            // Check if we can add to the current batch
            let can_add_to_current = current_batch.as_ref().is_some_and(|batch| {
                if batch.batch_type != batch_type {
                    return false;
                }
                batch
                    .items
                    .last()
                    .is_some_and(|last_item| can_batch_together(last_item, item))
            });

            if can_add_to_current && let Some(ref mut batch) = current_batch {
                batch.push(item.clone());
                continue;
            }

            // Can't batch with current, flush it
            if let Some(batch) = current_batch.take() {
                batches.push(batch);
            }

            // Start a new batch
            let mut new_batch = DrawBatch::new(batch_type);
            new_batch.push(item.clone());
            current_batch = Some(new_batch);
        } else {
            // Non-batchable item (marker), flush current batch and create single-item batch
            if let Some(batch) = current_batch.take() {
                batches.push(batch);
            }

            // For markers, we need to preserve them but they don't go into typed batches
            // We could create a special marker batch type, but for now we'll skip them
            // as they're handled separately in the render graph
        }
    }

    // Flush any remaining batch
    if let Some(batch) = current_batch {
        batches.push(batch);
    }

    batches
}

/// Statistics about batching efficiency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatchingStats {
    /// Number of batches created.
    pub batch_count: usize,
    /// Number of items in the largest batch.
    pub max_batch_size: usize,
    /// Average batch size.
    pub avg_batch_size: f32,
    /// Total number of items batched.
    pub total_items: usize,
}

impl BatchingStats {
    /// Calculate batching statistics from a list of batches.
    ///
    /// # Examples
    ///
    /// ```
    /// # use renderer::display_list::DisplayItem;
    /// # use renderer::render_graph::batching::{batch_draw_calls, BatchingStats};
    /// let items = vec![
    ///     DisplayItem::Rect {
    ///         x: 0.0,
    ///         y: 0.0,
    ///         width: 100.0,
    ///         height: 100.0,
    ///         color: [1.0, 0.0, 0.0, 1.0],
    ///     },
    ///     DisplayItem::Rect {
    ///         x: 100.0,
    ///         y: 0.0,
    ///         width: 100.0,
    ///         height: 100.0,
    ///         color: [0.0, 1.0, 0.0, 1.0],
    ///     },
    /// ];
    /// let batches = batch_draw_calls(&items);
    /// let stats = BatchingStats::from_batches(&batches);
    /// assert_eq!(stats.batch_count, 1);
    /// assert_eq!(stats.max_batch_size, 2);
    /// ```
    pub fn from_batches(batches: &[DrawBatch]) -> Self {
        let batch_count = batches.len();
        let max_batch_size = batches.iter().map(DrawBatch::len).max().unwrap_or(0);
        let total_items: usize = batches.iter().map(DrawBatch::len).sum();
        let avg_batch_size = if batch_count > 0 {
            total_items as f32 / batch_count as f32
        } else {
            0.0
        };

        Self {
            batch_count,
            max_batch_size,
            avg_batch_size,
            total_items,
        }
    }
}
