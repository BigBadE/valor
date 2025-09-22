//! Height adapter module for CSS 2.2 §10.6
//! Delegates to the existing Layouter height logic to keep behavior identical.

use crate::{HeightExtras, Layouter};
use js::NodeKey;
use style_engine::ComputedStyle;

/// Compute used border-box height for a non-replaced block per CSS 2.2 §10.6.
/// Delegates to the layouter's internal implementation to preserve behavior.
///
/// A public helper function for computing used height.
#[inline]
pub fn compute_used_height(
    layouter: &Layouter,
    style: &ComputedStyle,
    child_key: NodeKey,
    extras: HeightExtras,
    child_content_height: i32,
) -> i32 {
    Layouter::compute_used_height(layouter, style, child_key, extras, child_content_height)
}
