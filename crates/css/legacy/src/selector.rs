/// A simple selector component (type, id, class, universal, attribute, pseudo-class).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimpleSelector {
    /// Element tag name (type selector), e.g. `div`.
    Type(String),
    /// Id selector, e.g. `#main`.
    Id(String),
    /// Class selector, e.g. `.button`.
    Class(String),
    /// Universal selector (`*`).
    Universal,
    /// Attribute selector, e.g. `[href]` or `[type="button"]`. Only `=` operator supported for now.
    Attribute { name: String, op_value: Option<(String, String)> },
    /// Pseudo-class selectors subset.
    PseudoClass(PseudoClass),
}

/// Supported pseudo-classes in this minimal engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PseudoClass {
    Root,
    FirstChild,
    LastChild,
}

/// A combinator between two compound selectors within a complex selector.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Combinator {
    /// Descendant combinator (whitespace).
    Descendant,
    /// Child combinator (`>`).
    Child,
    /// Adjacent sibling combinator (`+`).
    NextSibling,
    /// General sibling combinator (`~`).
    SubsequentSibling,
}

/// A sequence of simple selectors with no combinators.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CompoundSelector {
    /// The list of simple selectors that form the compound.
    pub simples: Vec<SimpleSelector>,
}

/// A full selector possibly composed of multiple compound selectors joined by combinators.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ComplexSelector {
    /// Left-to-right sequence of (compound, combinator-to-next). The last combinator is None.
    pub sequence: Vec<(CompoundSelector, Option<Combinator>)>,
    /// Specificity of the selector.
    pub specificity: Specificity,
}

/// A numeric specificity value (higher = more specific), packed for sorting.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Default)]
pub struct Specificity(pub u32);

impl Specificity {
    /// Build a Specificity from counts of id, class/attr/pseudo-class, and type/pseudo-element selectors.
    pub fn from_counts(id_count: u32, class_like_count: u32, type_like_count: u32) -> Self {
        // id_count in high bits, then class_like_count, then type_like_count
        Specificity((id_count << 20) | (class_like_count << 10) | type_like_count)
    }
}

impl ComplexSelector {
    /// Return the rightmost compound selector if present.
    pub fn rightmost_compound(&self) -> Option<&CompoundSelector> {
        self.sequence.last().map(|(compound, _)| compound)
    }
}
