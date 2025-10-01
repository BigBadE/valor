//! Axis resolution and ordering utilities
//! Spec: <https://www.w3.org/TR/css-flexbox-1/#box-model>
//! Spec: <https://www.w3.org/TR/css-flexbox-1/#propdef-order>

use crate::chapter5::FlexDirection;
use crate::chapter6::ItemRef;

/// Minimal writing mode subset for axis resolution.
///
/// Spec: <https://www.w3.org/TR/css-writing-modes-4/#writing-mode>
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum WritingMode {
    /// Horizontal-tb: inline direction is horizontal; block direction is vertical.
    HorizontalTb,
    /// Vertical-rl: inline direction is vertical, block advances to the left.
    VerticalRl,
    /// Vertical-lr: inline direction is vertical, block advances to the right.
    VerticalLr,
}

/// Resolved axes information for a flex container.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Axes {
    /// True when the main axis maps to inline flow in `HorizontalTb` (row-wise layout)
    pub main_is_inline: bool,
    /// True when main axis is reversed (row-reverse or column-reverse)
    pub main_reverse: bool,
}

/// Resolve main/cross axes and direction given flex-direction and writing mode.
///
/// Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-direction-property>
pub const fn resolve_axes(direction: FlexDirection, writing_mode: WritingMode) -> Axes {
    match direction {
        FlexDirection::Row => Axes {
            main_is_inline: true,
            main_reverse: false,
        },
        FlexDirection::RowReverse => Axes {
            main_is_inline: true,
            main_reverse: true,
        },
        FlexDirection::Column => match writing_mode {
            WritingMode::HorizontalTb => Axes {
                main_is_inline: false,
                main_reverse: false,
            },
            WritingMode::VerticalRl | WritingMode::VerticalLr => Axes {
                main_is_inline: true,
                main_reverse: false,
            },
        },
        FlexDirection::ColumnReverse => match writing_mode {
            WritingMode::HorizontalTb => Axes {
                main_is_inline: false,
                main_reverse: true,
            },
            WritingMode::VerticalRl | WritingMode::VerticalLr => Axes {
                main_is_inline: true,
                main_reverse: true,
            },
        },
    }
}

/// Compute a stable ordering key for a flex item.
/// Returns (order, `original_index`) so a stable sort by this key respects DOM order ties.
///
/// Spec: <https://www.w3.org/TR/css-flexbox-1/#propdef-order>
pub const fn order_key(order: i32, original_index: usize) -> (i32, usize) {
    (order, original_index)
}

/// Stable sort of items by order, preserving input order for ties.
///
/// Spec: <https://www.w3.org/TR/css-flexbox-1/#order-property>
type OrderKey = (i32, usize);

pub fn sort_items_by_order_stable(items: &[(ItemRef, i32)]) -> Vec<ItemRef> {
    let mut with_index: Vec<(OrderKey, ItemRef)> = items
        .iter()
        .enumerate()
        .map(|(original_index, &(handle, order))| (order_key(order, original_index), handle))
        .collect();
    with_index.sort_by(|key_a, key_b| key_a.0.cmp(&key_b.0));
    with_index.into_iter().map(|(_, handle)| handle).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chapter5::FlexDirection;

    #[test]
    /// # Panics
    /// Panics if axes resolution does not match expected mapping for `HorizontalTb`.
    fn axes_horizontal_tb() {
        let axes_row = resolve_axes(FlexDirection::Row, WritingMode::HorizontalTb);
        assert_eq!(
            axes_row,
            Axes {
                main_is_inline: true,
                main_reverse: false
            }
        );
        let axes_row_rev = resolve_axes(FlexDirection::RowReverse, WritingMode::HorizontalTb);
        assert_eq!(
            axes_row_rev,
            Axes {
                main_is_inline: true,
                main_reverse: true
            }
        );
        let axes_col = resolve_axes(FlexDirection::Column, WritingMode::HorizontalTb);
        assert_eq!(
            axes_col,
            Axes {
                main_is_inline: false,
                main_reverse: false
            }
        );
        let axes_col_rev = resolve_axes(FlexDirection::ColumnReverse, WritingMode::HorizontalTb);
        assert_eq!(
            axes_col_rev,
            Axes {
                main_is_inline: false,
                main_reverse: true
            }
        );
    }

    #[test]
    /// # Panics
    /// Panics if stable order sorting does not preserve input order for ties.
    fn stable_order_sorting() {
        let items = vec![
            (ItemRef(10), 1i32),
            (ItemRef(11), 0i32),
            (ItemRef(12), 1i32),
            (ItemRef(13), 0i32),
        ];
        let sorted = sort_items_by_order_stable(&items);
        let got: Vec<u64> = sorted.iter().map(|handle| handle.0).collect();
        // Expect all order=0 first in original order (11, 13), then order=1 (10, 12) preserving input order within groups
        assert_eq!(got, vec![11, 13, 10, 12]);
    }
}
