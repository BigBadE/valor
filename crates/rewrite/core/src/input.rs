//! Input trait for external data.
//!
//! Inputs represent external data that queries depend on but don't compute themselves.
//! Examples: DOM properties, CSS declarations, user interactions.

use crate::{Database, Query, dependency::DependencyContext};
use std::hash::Hash;

/// Trait for input data sources.
///
/// Unlike Query, Input doesn't compute values - it retrieves externally-provided data.
pub trait Input: 'static {
    /// The key type used to identify this input.
    type Key: Clone + Hash + Eq + Send + Sync + 'static;

    /// The value type this input provides.
    type Value: Clone + Send + Sync + 'static;

    /// Name of this input for debugging.
    fn name() -> &'static str;

    /// Default value when input is not set.
    fn default_value(key: &Self::Key) -> Self::Value;
}

/// Query wrapper for inputs.
pub struct InputQuery<I: Input>(std::marker::PhantomData<I>);

impl<I: Input> Query for InputQuery<I> {
    type Key = I::Key;
    type Value = I::Value;

    fn execute(db: &Database, key: Self::Key, _ctx: &mut DependencyContext) -> Self::Value {
        db.get_input::<I>(&key)
            .unwrap_or_else(|| I::default_value(&key))
    }
}
