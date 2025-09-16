//! Stylesheet parser integration for the css orchestrator.

use crate::types::{Origin, Rule, Stylesheet};
use css_syntax::parse_stylesheet as parse_syntax_stylesheet;

/// Internal top-level state used when emitting `Rule`s from a parsed stylesheet.
struct EmitState {
    /// Origin of rules (UA/User/Author) applied to produced `Rule`s.
    origin: Origin,
    /// Next source order to assign; monotonically increases per produced rule.
    order: u32,
}

pub struct StylesheetStreamParser {
    /// Origin of the stylesheet rules (UA/User/Author).
    origin: Origin,
    /// Base source index for emitted rules.
    base_rule_idx: u32,
    /// Accumulated CSS text buffer.
    buf: String,
}

impl StylesheetStreamParser {
    #[inline]
    pub const fn new(origin: Origin, base_rule_idx: u32) -> Self {
        Self {
            origin,
            base_rule_idx,
            buf: String::new(),
        }
    }

    #[inline]
    pub fn push_chunk(&mut self, text: &str, _accum: &mut Stylesheet) {
        self.buf.push_str(text);
    }

    #[inline]
    pub fn finish_with_next(self) -> (Stylesheet, Self) {
        let (sheet, next_order) =
            parse_stylesheet_with_next(&self.buf, self.origin, self.base_rule_idx);
        let next = Self::new(self.origin, next_order);
        (sheet, next)
    }
}

#[inline]
/// Parse a stylesheet string and return the resulting `Stylesheet`.
pub fn parse_stylesheet(css: &str, origin: Origin, base_rule_idx: u32) -> Stylesheet {
    let (sheet, _next) = parse_stylesheet_with_next(css, origin, base_rule_idx);
    sheet
}

#[inline]
/// Parse a stylesheet string and return the resulting `Stylesheet` together with the next source
/// order that should be used for a subsequent parse.
fn parse_stylesheet_with_next(css: &str, origin: Origin, base_rule_idx: u32) -> (Stylesheet, u32) {
    let parsed = parse_syntax_stylesheet(css);
    let mut state = EmitState {
        origin,
        order: base_rule_idx,
    };
    let mut sheet = Stylesheet::with_origin(origin);
    for _style_rule in parsed.rules {
        let rule = Rule {
            origin: state.origin,
            source_order: state.order,
        };
        state.order = state.order.saturating_add(1);
        sheet.rules.push(rule);
    }
    (sheet, state.order)
}
