//! CSS parsing.

mod parser;
mod property_groups;
mod selectors;
mod style;
mod styler_context;
pub mod value_resolver;
pub use parser::{CssParser, ParsedRule, Properties};
pub use property_groups::{affects_position, affects_size};
pub use selectors::matches_selector_list;
pub use style::{ScopedStyler, Styler};
pub use styler_context::NodeStylerContext;

pub use lightningcss::properties::Property;
pub use lightningcss::properties::PropertyId;
pub use lightningcss::selector::SelectorList;

/// Concrete formula type parameterized by `NodeStylerContext`.
pub type Formula = rewrite_core::GenericFormula<NodeStylerContext<'static>>;

/// Concrete formula list type.
pub type FormulaList = rewrite_core::GenericFormulaList<NodeStylerContext<'static>>;

/// Concrete query function type.
pub type QueryFn = rewrite_core::QueryFn<NodeStylerContext<'static>>;
