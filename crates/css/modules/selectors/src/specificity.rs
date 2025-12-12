//! CSS selector specificity calculation.
//! Spec: <https://www.w3.org/TR/selectors-3/#specificity>

use crate::{ComplexSelector, CompoundSelector, SimpleSelector};

/// Specificity triple (a, b, c).
/// Spec: Section 13 — Calculating a selector's specificity
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Specificity(pub u16, pub u16, pub u16);

/// Compute the specificity of a compound selector.
/// Spec: Section 13 — Specificity (a, b, c)
pub fn specificity_of_compound(compound: &CompoundSelector) -> Specificity {
    let mut id_count = 0u16;
    let mut class_attr_count = 0u16;
    let mut type_count = 0u16;
    for simple in compound.simples.iter().cloned() {
        match simple {
            SimpleSelector::IdSelector(_) => {
                id_count = id_count.saturating_add(1);
            }
            SimpleSelector::Class(_) | SimpleSelector::AttrEquals { .. } => {
                class_attr_count = class_attr_count.saturating_add(1);
            }
            SimpleSelector::Type(name) => {
                if name.as_str() != "*" {
                    type_count = type_count.saturating_add(1);
                }
            }
            SimpleSelector::Universal => {}
        }
    }
    Specificity(id_count, class_attr_count, type_count)
}

/// Compute the specificity of a complex selector (sum of its compounds).
/// Spec: Section 13 — Specificity accumulation
pub fn specificity_of_complex(sel: &ComplexSelector) -> Specificity {
    let mut spec_total = specificity_of_compound(&sel.first);
    for pair in &sel.rest {
        let compound = &pair.1;
        let spec_add = specificity_of_compound(compound);
        spec_total.0 = spec_total.0.saturating_add(spec_add.0);
        spec_total.1 = spec_total.1.saturating_add(spec_add.1);
        spec_total.2 = spec_total.2.saturating_add(spec_add.2);
    }
    spec_total
}
