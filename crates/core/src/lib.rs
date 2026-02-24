#![feature(generic_const_exprs)]
#![allow(incomplete_features, reason = "generic_const_exprs is experimental")]

//! Core infrastructure for formula-based layout computation.

pub mod css;
pub mod db;
pub mod formula;
pub mod rayon_dispatch;
pub mod types;

// Re-export at crate root
pub use css::*;
pub use db::*;
pub use formula::{
    Aggregation, Formula, FormulaList, ImperativeFn, LineAggregateParams, LineItemAggregateParams,
    MeasureAxis, MeasureMode, Operation, PrevLinesAggregateParams, QueryFn, ResolveContext,
    StylerAccess, TextMeasurement,
};
pub use rayon_dispatch::rayon_dispatch;
pub use types::*;
