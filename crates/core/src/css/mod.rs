//! CSS property and value types.

pub mod subscriptions;

pub use subscriptions::{DomBroadcast, Subscriber, Subscriptions};

// Re-export lightningcss types
pub use lightningcss::properties::Property;

/// Specificity of a CSS selector with importance flag.
///
/// Cascade priority order: important > ids > classes > elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Specificity {
    pub important: bool,
    pub ids: u32,
    pub classes: u32,
    pub elements: u32,
}

impl Specificity {
    pub const fn new(ids: u32, classes: u32, elements: u32) -> Self {
        Self {
            important: false,
            ids,
            classes,
            elements,
        }
    }

    pub const fn important(ids: u32, classes: u32, elements: u32) -> Self {
        Self {
            important: true,
            ids,
            classes,
            elements,
        }
    }

    pub const ZERO: Self = Self::new(0, 0, 0);
    pub const INLINE: Self = Self {
        important: false,
        ids: u32::MAX,
        classes: 0,
        elements: 0,
    };
    pub const INLINE_IMPORTANT: Self = Self {
        important: true,
        ids: u32::MAX,
        classes: 0,
        elements: 0,
    };

    /// Return a copy with importance set.
    pub const fn with_important(self, important: bool) -> Self {
        Self { important, ..self }
    }
}

impl PartialOrd for Specificity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Specificity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.important
            .cmp(&other.important)
            .then(self.ids.cmp(&other.ids))
            .then(self.classes.cmp(&other.classes))
            .then(self.elements.cmp(&other.elements))
    }
}

impl From<(u32, u32, u32)> for Specificity {
    fn from((ids, classes, elements): (u32, u32, u32)) -> Self {
        Self {
            important: false,
            ids,
            classes,
            elements,
        }
    }
}
