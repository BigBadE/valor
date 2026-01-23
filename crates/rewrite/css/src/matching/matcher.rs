//! Query for matching CSS selectors against elements.

use super::element_wrapper::{ElementWrapper, SelectorImpl};
use super::stylesheet::StyleSheetsInput;
use cssparser::{Parser, ParserInput};
use rewrite_core::{Database, DependencyContext, NodeId, Query};
use selectors::NthIndexCache;
use selectors::matching::{
    IgnoreNthChildForInvalidation, MatchingContext, MatchingMode, NeedsSelectorFlags, QuirksMode,
    matches_selector,
};
use selectors::parser::SelectorList;
use std::cell::RefCell;

/// Query that finds all CSS rules matching a given element.
/// Returns rule indices sorted by specificity (lowest to highest).
pub struct MatchedRulesQuery;

impl Query for MatchedRulesQuery {
    type Key = NodeId;
    type Value = Vec<usize>; // Indices into StyleSheets.rules

    fn execute(db: &Database, node: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        let stylesheets = db.get_input::<StyleSheetsInput>(&()).unwrap_or_default();
        let mut matched_indices = Vec::new();

        for (idx, rule) in stylesheets.rules.iter().enumerate() {
            if selector_matches(&rule.selector_text, node, db, ctx) {
                matched_indices.push(idx);
            }
        }

        matched_indices.sort_by_key(|&idx| {
            let rule = &stylesheets.rules[idx];
            (rule.specificity, rule.source_order)
        });

        matched_indices
    }
}

/// Check if a selector matches an element.
fn selector_matches(
    selector_text: &str,
    node: NodeId,
    db: &Database,
    ctx: &mut DependencyContext,
) -> bool {
    let mut input = ParserInput::new(selector_text);
    let mut parser = Parser::new(&mut input);

    let selector_list = match SelectorList::<SelectorImpl>::parse(
        &super::SelectorParser,
        &mut parser,
        selectors::parser::ParseRelative::No,
    ) {
        Ok(list) => list,
        Err(_) => return false,
    };

    let ctx_cell = RefCell::new(std::mem::replace(ctx, DependencyContext::new()));
    let element = ElementWrapper::new(node, db, &ctx_cell);

    let mut nth_index_cache = NthIndexCache::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut nth_index_cache,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        IgnoreNthChildForInvalidation::No,
    );

    let result = selector_list
        .0
        .iter()
        .any(|selector| matches_selector(selector, 0, None, &element, &mut context));

    *ctx = ctx_cell.into_inner();
    result
}

/// Calculate specificity for a selector.
pub fn calculate_specificity(selector_text: &str) -> u32 {
    let mut input = ParserInput::new(selector_text);
    let mut parser = Parser::new(&mut input);

    let selector_list = match SelectorList::<SelectorImpl>::parse(
        &super::SelectorParser,
        &mut parser,
        selectors::parser::ParseRelative::No,
    ) {
        Ok(list) => list,
        Err(_) => return 0,
    };

    selector_list
        .0
        .iter()
        .map(|selector| selector.specificity())
        .max()
        .unwrap_or(0)
}
