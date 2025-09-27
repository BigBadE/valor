//! Flex Items — identification and lightweight model
//! Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-items>

/// Minimal handle for an item reference. This crate keeps it opaque.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ItemRef(pub u64);

/// Minimal style subset needed for flex item collection.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ItemStyle {
    /// Equivalent to `display: none` check upstream; when true, item is skipped.
    pub is_none: bool,
    /// Whether the item is out-of-flow (e.g., absolute/fixed). Skipped for flex item collection.
    pub out_of_flow: bool,
}

/// Flex item shell combining reference and style for downstream layout.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FlexItem {
    pub handle: ItemRef,
}

/// Collect in-flow flex items from normalized children.
///
/// Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-items>
#[inline]
pub fn collect_flex_items(children: &[(ItemRef, ItemStyle)]) -> Vec<FlexItem> {
    let mut out = Vec::with_capacity(children.len());
    for (handle, style) in children.iter().copied() {
        if is_flex_item(style) {
            out.push(FlexItem { handle });
        }
    }
    out
}

/// Returns true when the child qualifies as a flex item in-flow.
///
/// Behavior:
/// - Excludes `display: none` (represented by `is_none`).
/// - Excludes out-of-flow boxes (absolute/fixed) represented by `out_of_flow`.
/// - Assumes children list is already normalized (e.g., `display: contents` handled upstream).
///
/// Spec: <https://www.w3.org/TR/css-flexbox-1/#flex-items>
#[inline]
pub const fn is_flex_item(style: ItemStyle) -> bool {
    !style.is_none && !style.out_of_flow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// # Panics
    /// Panics if filtering does not exclude `display:none` and out-of-flow children.
    fn collects_only_in_flow_non_none_children() {
        let inflow_one = (
            ItemRef(1),
            ItemStyle {
                is_none: false,
                out_of_flow: false,
            },
        );
        let none_item = (
            ItemRef(2),
            ItemStyle {
                is_none: true,
                out_of_flow: false,
            },
        );
        let absolute_item = (
            ItemRef(3),
            ItemStyle {
                is_none: false,
                out_of_flow: true,
            },
        );
        let inflow_two = (
            ItemRef(4),
            ItemStyle {
                is_none: false,
                out_of_flow: false,
            },
        );
        let items = collect_flex_items(&[inflow_one, none_item, absolute_item, inflow_two]);
        let handles: Vec<u64> = items.iter().map(|item| item.handle.0).collect();
        assert_eq!(handles, vec![1, 4]);
    }
}
