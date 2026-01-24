//! CSS property inheritance.
//!
//! This module now re-exports CascadedPropertyQuery as InheritedCssPropertyQuery
//! for backward compatibility. The cascade query already handles inheritance.

use super::cascade::CascadedPropertyQuery;

/// Query for CSS property values with inheritance support.
///
/// This is now an alias to CascadedPropertyQuery, which implements the full cascade
/// including user agent, author, and inline styles, with proper inheritance.
pub type InheritedCssPropertyQuery = CascadedPropertyQuery;
