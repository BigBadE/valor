//! Size mode types and dispatcher traits.
//!
//! This crate defines the SizeMode enum and dispatcher traits that layout
//! modules implement to provide sizing without circular dependencies.

use rewrite_core::ScopedDb;
use rewrite_css::Subpixels;
use rewrite_layout_util::Axis;
use std::marker::PhantomData;

/// Size mode enumeration - specifies constrained or intrinsic sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum SizeMode {
    /// Constrained size (based on available space)
    Constrained,
    /// Intrinsic size (min-content or max-content)
    Intrinsic,
}

/// Dispatcher trait for flex layout sizing.
///
/// The flex crate implements this trait, and the size crate uses it
/// to compute flex container sizes without depending on the flex crate directly.
pub trait FlexSizeDispatcher: 'static {
    /// Compute the size of a flex container using a generic size dispatcher.
    fn compute_flex_size<D>(scoped: &mut ScopedDb, axis: Axis, mode: SizeMode) -> Subpixels
    where
        D: SizeDispatcher + 'static;
}

/// Main size dispatcher trait.
///
/// This is implemented by the SizeDispatcher in the size crate.
pub trait SizeDispatcher {
    /// Query the size along an axis with a given mode.
    fn query(scoped: &mut ScopedDb, axis: Axis, mode: SizeMode) -> Subpixels;
}

/// Size query that takes a dispatcher and flex implementation.
///
/// This allows the flex crate to query child sizes without circular dependencies.
/// The dispatcher D is passed through from the calling context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchedSizeQuery<AxisParam, ModeParam, D, FlexImpl>
where
    D: SizeDispatcher,
    FlexImpl: FlexSizeDispatcher,
{
    _phantom: PhantomData<(AxisParam, ModeParam, D, FlexImpl)>,
}

impl<AxisParam, ModeParam, D, FlexImpl> rewrite_core::Query
    for DispatchedSizeQuery<AxisParam, ModeParam, D, FlexImpl>
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
    ModeParam: SizeModeMarker + 'static,
    D: SizeDispatcher + 'static,
    FlexImpl: FlexSizeDispatcher + 'static,
{
    type Key = rewrite_core::NodeId;
    type Value = Subpixels;

    fn execute(
        db: &rewrite_core::Database,
        node: rewrite_core::NodeId,
        ctx: &mut rewrite_core::DependencyContext,
    ) -> Self::Value {
        let mut scoped = rewrite_core::ScopedDb::new(db, node, ctx);
        let axis = AxisParam::to_value();
        let mode = ModeParam::to_value();
        D::query(&mut scoped, axis, mode)
    }
}
