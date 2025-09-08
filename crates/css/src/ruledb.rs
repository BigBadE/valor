use crate::selector::{ComplexSelector, SimpleSelector};
use crate::types::{Declaration, Origin, Stylesheet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightmostKey {
    Id(String),
    Class(String),
    Tag(String),
    Universal,
}

#[derive(Clone, Debug)]
pub struct RuleEntry {
    pub origin: Origin,
    pub source_order: u32,
    pub selector: ComplexSelector,
    pub declarations: Vec<Declaration>,
    /// Cached specificity copied from selector
    pub specificity: crate::selector::Specificity,
    /// Cached rightmost key to speed up indexing/matching in future phases
    pub rightmost: RightmostKey,
    /// Optional back-reference to original (rule_idx, selector_idx) for debugging
    pub original_idx: Option<(usize, usize)>,
}

#[derive(Clone, Debug, Default)]
pub struct RuleDB {
    pub entries: Vec<RuleEntry>,
}

impl RuleDB {
    /// Flatten a Stylesheet into a set of RuleEntry values, one per selector of each rule.
    pub fn from_stylesheet(sheet: &Stylesheet) -> Self {
        let mut db = RuleDB { entries: Vec::with_capacity(sheet.rules.len()) };
        for (ri, rule) in sheet.rules.iter().enumerate() {
            for (si, sel) in rule.selectors.iter().enumerate() {
                let rightmost = rightmost_key_for(sel);
                db.entries.push(RuleEntry {
                    origin: rule.origin,
                    source_order: rule.source_order,
                    selector: sel.clone(),
                    declarations: rule.declarations.clone(),
                    specificity: sel.specificity,
                    rightmost,
                    original_idx: Some((ri, si)),
                });
            }
        }
        db
    }
}

fn rightmost_key_for(sel: &ComplexSelector) -> RightmostKey {
    if let Some(comp) = sel.rightmost_compound() {
        // Prefer id > class > tag; if none, universal
        for s in &comp.simples {
            if let SimpleSelector::Id(id) = s { return RightmostKey::Id(id.clone()); }
        }
        for s in &comp.simples {
            if let SimpleSelector::Class(c) = s { return RightmostKey::Class(c.clone()); }
        }
        for s in &comp.simples {
            if let SimpleSelector::Type(t) = s { return RightmostKey::Tag(t.clone()); }
        }
    }
    RightmostKey::Universal
}
