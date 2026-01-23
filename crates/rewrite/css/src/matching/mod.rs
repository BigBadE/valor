//! CSS selector matching and rule application.

mod element_wrapper;
mod matcher;
mod style_parser;
mod stylesheet;

pub use matcher::{MatchedRulesQuery, calculate_specificity};
pub use style_parser::parse_css;
pub use stylesheet::{StyleRule, StyleSheets, StyleSheetsInput};

use cssparser::Parser;
use element_wrapper::{AttrString, NonTSPseudoClass, PseudoElement, SelectorImpl};
use selectors::parser::SelectorParseErrorKind;

/// Parser for CSS selectors.
pub struct SelectorParser;

impl<'i> selectors::parser::Parser<'i> for SelectorParser {
    type Impl = SelectorImpl;
    type Error = SelectorParseErrorKind<'i>;

    fn parse_non_ts_pseudo_class(
        &self,
        _location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<NonTSPseudoClass, cssparser::ParseError<'i, SelectorParseErrorKind<'i>>> {
        Err(
            _location.new_custom_error(SelectorParseErrorKind::UnsupportedPseudoClassOrElement(
                name,
            )),
        )
    }

    fn parse_pseudo_element(
        &self,
        _location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoElement, cssparser::ParseError<'i, SelectorParseErrorKind<'i>>> {
        Err(
            _location.new_custom_error(SelectorParseErrorKind::UnsupportedPseudoClassOrElement(
                name,
            )),
        )
    }

    fn parse_non_ts_functional_pseudo_class<'t>(
        &self,
        name: cssparser::CowRcStr<'i>,
        _parser: &mut Parser<'i, 't>,
    ) -> Result<NonTSPseudoClass, cssparser::ParseError<'i, SelectorParseErrorKind<'i>>> {
        Err(
            _parser.new_custom_error(SelectorParseErrorKind::UnsupportedPseudoClassOrElement(
                name,
            )),
        )
    }

    fn default_namespace(&self) -> Option<()> {
        None
    }

    fn namespace_for_prefix(&self, _prefix: &AttrString) -> Option<()> {
        None
    }
}
