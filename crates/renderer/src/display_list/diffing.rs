//! Fine-grained display list diffing for incremental updates.
//!
//! This module provides algorithms to compute minimal deltas between display lists,
//! enabling partial redraws and reduced GPU command submission overhead.

use super::core::DisplayItem;

/// A delta operation describing how to transform one display list into another.
///
/// This enum represents atomic operations that can be applied to transform an old
/// display list into a new one with minimal changes.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayListDelta {
    /// Insert a new item at the specified index.
    ///
    /// The item will be inserted before the current item at `index`.
    /// If `index` equals the list length, the item is appended.
    Insert {
        /// Zero-based index where the item should be inserted
        index: usize,
        /// The display item to insert
        item: DisplayItem,
    },
    /// Remove an item at the specified index.
    ///
    /// Items after this index will shift down by one position.
    Remove {
        /// Zero-based index of the item to remove
        index: usize,
    },
    /// Update an existing item at the specified index.
    ///
    /// The old item is replaced with the new item.
    Update {
        /// Zero-based index of the item to update
        index: usize,
        /// The new display item that replaces the old one
        item: DisplayItem,
    },
}

/// Compute a minimal delta between two display lists.
///
/// This function implements a simple O(n) linear scan diffing algorithm.
/// For MVP purposes, it identifies insertions, deletions, and updates by
/// comparing items at the same index.
///
/// # Algorithm
///
/// The algorithm works as follows:
/// 1. Compare items at matching indices up to `min(old.len(), new.len())`
/// 2. Generate `Update` deltas for changed items
/// 3. Generate `Remove` deltas for items only in the old list
/// 4. Generate `Insert` deltas for items only in the new list
///
/// # Future Optimizations
///
/// This simple algorithm can be enhanced with:
/// - **Longest Common Subsequence (LCS)**: Better move detection
/// - **Myers' diff algorithm**: Industry-standard diffing with O(nd) complexity
/// - **Tree diffing**: Exploit the hierarchical structure of display lists
///
/// # Examples
///
/// ```
/// # use renderer::DisplayItem;
/// # use renderer::display_list::diffing::diff_display_lists;
/// let old_items = vec![
///     DisplayItem::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0, color: [1.0, 0.0, 0.0, 1.0] },
/// ];
/// let new_items = vec![
///     DisplayItem::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0, color: [0.0, 1.0, 0.0, 1.0] },
/// ];
/// let deltas = diff_display_lists(&old_items, &new_items);
/// assert_eq!(deltas.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn diff_display_lists(old: &[DisplayItem], new: &[DisplayItem]) -> Vec<DisplayListDelta> {
    let mut deltas: Vec<DisplayListDelta> = Vec::new();

    let min_len = old.len().min(new.len());

    // Compare items at matching indices and generate updates
    for idx in 0..min_len {
        if old[idx] != new[idx] {
            deltas.push(DisplayListDelta::Update {
                index: idx,
                item: new[idx].clone(),
            });
        }
    }

    // Handle removals (old list is longer)
    if old.len() > new.len() {
        // Remove from the end to avoid index shifting issues
        for idx in (new.len()..old.len()).rev() {
            deltas.push(DisplayListDelta::Remove { index: idx });
        }
    }

    // Handle insertions (new list is longer)
    if new.len() > old.len() {
        for (offset, item) in new.iter().enumerate().skip(old.len()) {
            deltas.push(DisplayListDelta::Insert {
                index: offset,
                item: item.clone(),
            });
        }
    }

    deltas
}

/// Apply a sequence of deltas to a display list in place.
///
/// This function modifies the provided vector of display items by applying
/// each delta operation in order.
///
/// # Panics
///
/// This function will panic if:
/// - An `Insert` delta has an `index` greater than the current list length
/// - A `Remove` or `Update` delta has an `index` out of bounds
///
/// # Examples
///
/// ```
/// # use renderer::DisplayItem;
/// # use renderer::display_list::diffing::{DisplayListDelta, apply_deltas};
/// let mut items = vec![
///     DisplayItem::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0, color: [1.0, 0.0, 0.0, 1.0] },
/// ];
/// let deltas = vec![
///     DisplayListDelta::Update {
///         index: 0,
///         item: DisplayItem::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0, color: [0.0, 1.0, 0.0, 1.0] },
///     },
/// ];
/// apply_deltas(&mut items, &deltas);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn apply_deltas(items: &mut Vec<DisplayItem>, deltas: &[DisplayListDelta]) {
    for delta in deltas {
        match delta {
            DisplayListDelta::Insert { index, item } => {
                items.insert(*index, item.clone());
            }
            DisplayListDelta::Remove { index } => {
                let _: DisplayItem = items.remove(*index);
            }
            DisplayListDelta::Update { index, item } => {
                items[*index] = item.clone();
            }
        }
    }
}
