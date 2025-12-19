//! HTML representation for reactive components

use std::fmt;

/// Represents HTML content that can be rendered
#[derive(Clone, Debug)]
pub struct Html {
    /// Raw HTML string
    pub content: String,
}

impl Html {
    /// Create HTML from a string
    #[inline]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }

    /// Create empty HTML
    #[inline]
    pub fn empty() -> Self {
        Self {
            content: String::new(),
        }
    }
}

impl From<String> for Html {
    #[inline]
    fn from(content: String) -> Self {
        Self::new(content)
    }
}

impl From<&str> for Html {
    #[inline]
    fn from(content: &str) -> Self {
        Self::new(content)
    }
}

impl fmt::Display for Html {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.content)
    }
}
