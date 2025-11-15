//! Display list serialization for debugging and testing.
//!
//! This module provides binary serialization of display lists using bincode.
//! Useful for:
//! - Recording/replay for debugging
//! - Saving display lists for regression testing
//! - Network transmission in remote rendering scenarios

use super::DisplayList;
use anyhow::{Error as AnyhowError, Result as AnyResult};
use bincode::{deserialize, serialize};

/// Serialize a display list to binary format using bincode.
///
/// # Errors
/// Returns an error if serialization fails.
///
/// # Example
/// ```
/// use renderer::display_list::{DisplayList, DisplayItem};
/// use renderer::display_list::serialization::serialize_display_list;
///
/// let mut list = DisplayList::new();
/// list.items.push(DisplayItem::Rect {
///     x: 0.0,
///     y: 0.0,
///     width: 100.0,
///     height: 100.0,
///     color: [1.0, 0.0, 0.0, 1.0],
/// });
/// let bytes = serialize_display_list(&list).unwrap();
/// assert!(!bytes.is_empty());
/// ```
pub fn serialize_display_list(list: &DisplayList) -> AnyResult<Vec<u8>> {
    serialize(list).map_err(|err| AnyhowError::msg(format!("Serialization failed: {err}")))
}

/// Deserialize a display list from binary format.
///
/// # Errors
/// Returns an error if deserialization fails or the data is corrupted.
///
/// # Example
/// ```
/// use renderer::display_list::{DisplayList, DisplayItem};
/// use renderer::display_list::serialization::{serialize_display_list, deserialize_display_list};
///
/// let mut list = DisplayList::new();
/// list.items.push(DisplayItem::Rect {
///     x: 0.0,
///     y: 0.0,
///     width: 100.0,
///     height: 100.0,
///     color: [1.0, 0.0, 0.0, 1.0],
/// });
/// let bytes = serialize_display_list(&list).unwrap();
/// let deserialized = deserialize_display_list(&bytes).unwrap();
/// assert_eq!(list, deserialized);
/// ```
pub fn deserialize_display_list(bytes: &[u8]) -> AnyResult<DisplayList> {
    deserialize(bytes).map_err(|err| AnyhowError::msg(format!("Deserialization failed: {err}")))
}
