//! Streaming CSS parser using lightningcss.
//!
//! Uses rayon for CPU-bound parsing work, designed to be driven by tokio for I/O.
//! Each chunk is processed asynchronously: tokio sends to rayon, awaits completion,
//! then proceeds to the next chunk.

use lasso::ThreadedRodeo;
use lightningcss::declaration::DeclarationBlock;
use lightningcss::properties::Property;
use lightningcss::rules::CssRule;
use lightningcss::selector::SelectorList;
use lightningcss::stylesheet::{ParserOptions, StyleSheet};
use lightningcss::traits::IntoOwned;
use rewrite_core::rayon_dispatch;
use std::sync::Arc;

use lightningcss::properties::PropertyId;

/// Owned properties extracted from a declaration block.
#[derive(Debug, Clone, Default)]
pub struct Properties {
    pub normal: Vec<Property<'static>>,
    pub important: Vec<Property<'static>>,
}

impl Properties {
    /// Check if this property set contains a property with the given ID.
    pub fn has_property(&self, id: &PropertyId<'static>) -> bool {
        self.normal.iter().any(|p| p.property_id() == *id)
            || self.important.iter().any(|p| p.property_id() == *id)
    }

    /// Check if this property set contains an important property with the given ID.
    pub fn has_important(&self, id: &PropertyId<'static>) -> bool {
        self.important.iter().any(|p| p.property_id() == *id)
    }
}

impl From<DeclarationBlock<'_>> for Properties {
    fn from(decls: DeclarationBlock<'_>) -> Self {
        Self {
            normal: expand_shorthands(decls.declarations),
            important: expand_shorthands(decls.important_declarations),
        }
    }
}

/// Expand shorthand properties into their longhand equivalents.
/// Properties that aren't shorthands are kept as-is.
fn expand_shorthands(props: Vec<Property<'_>>) -> Vec<Property<'static>> {
    let mut result = Vec::with_capacity(props.len());
    for prop in props {
        let owned = prop.into_owned();
        let prop_id = owned.property_id();

        if let Some(longhands) = prop_id.longhands() {
            for longhand_id in &longhands {
                if let Some(longhand) = owned.longhand(longhand_id) {
                    result.push(longhand.into_owned());
                }
            }
        } else {
            result.push(owned);
        }
    }
    result
}

/// A parsed CSS rule with owned data.
#[derive(Debug, Clone)]
pub enum ParsedRule {
    /// A stylesheet rule with selectors.
    Stylesheet {
        selectors: SelectorList<'static>,
        properties: Properties,
    },
    /// An inline style rule targeting a specific node.
    Inline {
        node_id: rewrite_core::NodeId,
        properties: Properties,
    },
}

impl ParsedRule {
    /// Check if this rule applies to the given node.
    pub fn matches(&self, node_id: rewrite_core::NodeId, tree: &rewrite_html::DomTree) -> bool {
        match self {
            Self::Stylesheet { selectors, .. } => {
                crate::matches_selector_list(tree, node_id, selectors)
            }
            Self::Inline {
                node_id: target, ..
            } => *target == node_id,
        }
    }

    /// Get the properties for this rule.
    pub fn properties(&self) -> &Properties {
        match self {
            Self::Stylesheet { properties, .. } | Self::Inline { properties, .. } => properties,
        }
    }

    /// Get the base specificity for this rule (without importance flag).
    pub fn specificity(&self) -> rewrite_core::Specificity {
        match self {
            Self::Stylesheet { selectors, .. } => {
                let spec = selectors
                    .0
                    .iter()
                    .map(|s| s.specificity())
                    .max()
                    .unwrap_or(0);
                let ids = (spec >> 20) & 0x3FF;
                let classes = (spec >> 10) & 0x3FF;
                let elements = spec & 0x3FF;
                rewrite_core::Specificity::new(ids, classes, elements)
            }
            Self::Inline { .. } => rewrite_core::Specificity::INLINE,
        }
    }
}

/// Streaming CSS parser that uses rayon for parsing.
///
/// Call `push_chunk` to add CSS text (awaits until rayon finishes parsing),
/// then `finish` when done.
pub struct CssParser<F> {
    buffer: String,
    callback: Arc<F>,
    #[allow(dead_code)]
    interner: Arc<ThreadedRodeo>,
}

impl<F: Fn(ParsedRule) + Send + Sync + 'static> CssParser<F> {
    /// Create a new CSS parser with the given callback.
    pub fn new(callback: F, interner: Arc<ThreadedRodeo>) -> Self {
        Self {
            buffer: String::new(),
            callback: Arc::new(callback),
            interner,
        }
    }

    /// Add a chunk of CSS text and parse on rayon. Awaits until parsing completes.
    pub async fn push_chunk(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);

        // Take ownership of buffer, get it back after parsing (avoids clone)
        let mut buffer = std::mem::take(&mut self.buffer);
        let callback = self.callback.clone();

        self.buffer = rayon_dispatch(move || {
            let consumed = parse_and_emit(&buffer, callback.as_ref(), true);
            buffer.drain(..consumed);
            buffer
        })
        .await;
    }

    /// Finish parsing, processing any remaining CSS in the buffer.
    pub async fn finish(self) {
        if self.buffer.is_empty() {
            return;
        }

        let buffer = self.buffer;
        let callback = self.callback.clone();

        rayon_dispatch(move || {
            parse_and_emit(&buffer, callback.as_ref(), false);
        })
        .await;
    }
}

/// Parse CSS text and invoke callback for each rule. Returns bytes consumed.
fn parse_and_emit<F: Fn(ParsedRule)>(css_text: &str, callback: &F, error_recovery: bool) -> usize {
    let options = ParserOptions {
        error_recovery,
        ..Default::default()
    };

    let Ok(stylesheet) = StyleSheet::parse(css_text, options) else {
        return 0;
    };

    let mut rules = Vec::new();
    let mut last_loc = None;

    for rule in stylesheet.rules.0 {
        if let CssRule::Style(style_rule) = rule {
            last_loc = Some(style_rule.loc);
            rules.push(ParsedRule::Stylesheet {
                selectors: style_rule.selectors.into_owned(),
                properties: style_rule.declarations.into(),
            });
        }
    }

    // Invoke callbacks
    for rule in rules {
        callback(rule);
    }

    // Calculate bytes consumed
    last_loc
        .map(|loc| line_col_to_byte(css_text, loc.line, loc.column))
        .unwrap_or(0)
}

fn line_col_to_byte(s: &str, line: u32, col: u32) -> usize {
    let mut current_line = 0u32;
    let mut current_col = 1u32;

    for (i, c) in s.char_indices() {
        if current_line == line && current_col == col {
            return i;
        }

        if c == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += c.len_utf16() as u32;
        }
    }

    s.len()
}
