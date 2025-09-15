use std::collections::HashMap;

use crate::selector::{ComplexSelector, SimpleSelector};
use crate::types::Stylesheet;

#[derive(Clone, Debug, Default)]
pub struct RuleMap {
    pub by_id: HashMap<String, Vec<RuleRef>>,
    pub by_class: HashMap<String, Vec<RuleRef>>,
    pub by_tag: HashMap<String, Vec<RuleRef>>,
    pub universal: Vec<RuleRef>,
}

#[derive(Copy, Clone, Debug)]
pub struct RuleRef {
    pub rule_idx: usize,
    pub selector_idx: usize,
}

impl RuleMap {
    pub fn new() -> Self { Self::default() }
}

pub fn index_rules(sheet: &Stylesheet, map: &mut RuleMap) {
    for (ri, rule) in sheet.rules.iter().enumerate() {
        for (si, sel) in rule.selectors.iter().enumerate() {
            index_selector(ri, si, sel, map);
        }
    }
}

fn index_selector(rule_idx: usize, selector_idx: usize, sel: &ComplexSelector, map: &mut RuleMap) {
    let Some(comp) = sel.rightmost_compound() else {
        map.universal.push(RuleRef { rule_idx, selector_idx });
        return;
    };

    // Prefer id > class > tag; if none found, add to universal
    // If multiple of a category exist, add duplicates (cheap, filtered during matching).
    let mut added = false;

    for s in &comp.simples {
        if let SimpleSelector::Id(id) = s {
            map.by_id.entry(id.clone()).or_default().push(RuleRef { rule_idx, selector_idx });
            added = true;
        }
    }
    if added { return; }

    for s in &comp.simples {
        if let SimpleSelector::Class(class) = s {
            map.by_class.entry(class.clone()).or_default().push(RuleRef { rule_idx, selector_idx });
            added = true;
        }
    }
    if added { return; }

    for s in &comp.simples {
        if let SimpleSelector::Type(tag) = s {
            map.by_tag.entry(tag.clone()).or_default().push(RuleRef { rule_idx, selector_idx });
            added = true;
        }
    }

    if !added {
        map.universal.push(RuleRef { rule_idx, selector_idx });
    }
}
