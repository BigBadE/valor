#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimpleSelector {
    Type(String),
    Id(String),
    Class(String),
    Universal,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CompoundSelector {
    pub simples: Vec<SimpleSelector>,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ComplexSelector {
    // Left-to-right sequence of (compound, combinator-to-next). The last combinator is None.
    pub sequence: Vec<(CompoundSelector, Option<Combinator>)>,
    pub specificity: Specificity,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Default)]
pub struct Specificity(pub u32);

impl Specificity {
    pub fn from_counts(a: u32, b: u32, c: u32) -> Self {
        // a in high byte, then b, then c
        Specificity((a << 20) | (b << 10) | c)
    }
}

impl ComplexSelector {
    pub fn rightmost_compound(&self) -> Option<&CompoundSelector> {
        self.sequence.last().map(|(c, _)| c)
    }
}
