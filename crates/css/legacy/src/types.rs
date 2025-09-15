use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Origin {
    UA = 0,
    User = 1,
    Author = 2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
    /// Custom property references captured from the value (e.g., "--main" in var(--main)).
    pub var_refs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StyleRule {
    pub selectors: Vec<crate::selector::ComplexSelector>,
    pub declarations: Vec<Declaration>,
    /// source order for stability in cascade
    pub source_order: u32,
    pub origin: Origin,
}

#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<StyleRule>,
}

impl fmt::Display for Declaration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.important {
            write!(f, "{}: {} !important", self.name, self.value)
        } else {
            write!(f, "{}: {}", self.name, self.value)
        }
    }
}