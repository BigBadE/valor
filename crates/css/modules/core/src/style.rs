//! Minimal style system stub used by the core engine.
//! Maintains a `Stylesheet` and a small computed styles map for the root node.

use std::collections::HashMap;

use crate::{style_model, types};
use css_style_attr::parse_style_attribute_into_map;
use css_variables::{CustomProperties, extract_custom_properties};
use js::{DOMUpdate, NodeKey};

/// Tracks stylesheet state and a tiny computed styles cache.
pub struct StyleComputer {
    /// The active stylesheet applied to the document.
    sheet: types::Stylesheet,
    /// Snapshot of computed styles (currently only the root is populated).
    computed: HashMap<NodeKey, style_model::ComputedStyle>,
    /// Whether the last recompute changed any styles.
    style_changed: bool,
    /// Nodes whose styles changed in the last recompute.
    changed_nodes: Vec<NodeKey>,
    /// Parsed inline style attribute declarations per node (author origin).
    inline_decls_by_node: HashMap<NodeKey, HashMap<String, String>>,
    /// Extracted custom properties (variables) per node for quick lookup.
    inline_custom_props_by_node: HashMap<NodeKey, CustomProperties>,
}

impl StyleComputer {
    /// Create a new style computer with an empty stylesheet and cache.
    #[inline]
    pub fn new() -> Self {
        Self {
            sheet: types::Stylesheet::default(),
            computed: HashMap::new(),
            style_changed: false,
            changed_nodes: Vec::new(),
            inline_decls_by_node: HashMap::new(),
            inline_custom_props_by_node: HashMap::new(),
        }
    }

    /// Replace the active stylesheet.
    #[inline]
    pub fn replace_stylesheet(&mut self, sheet: types::Stylesheet) {
        self.sheet = sheet;
    }

    /// Recompute dirty styles and return whether styles changed.
    #[inline]
    pub fn recompute_dirty(&mut self) -> bool {
        if self.computed.is_empty() {
            self.computed.insert(
                NodeKey::ROOT,
                style_model::ComputedStyle {
                    font_size: 16.0,
                    ..Default::default()
                },
            );
            self.style_changed = true;
            self.changed_nodes = vec![NodeKey::ROOT];
        } else {
            self.style_changed = false;
            self.changed_nodes.clear();
        }
        self.style_changed
    }

    /// Return a shallow copy of the current computed styles map.
    #[inline]
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, style_model::ComputedStyle> {
        self.computed.clone()
    }

    /// Apply a DOM update to the style system.
    /// Marks styles as dirty so a subsequent recompute can refresh caches.
    #[inline]
    pub fn apply_update(&mut self, update: DOMUpdate) {
        use DOMUpdate::{EndOfDocument, InsertElement, InsertText, RemoveNode, SetAttr};
        match update {
            SetAttr { node, name, value } => {
                if name.eq_ignore_ascii_case("style") {
                    let decls = parse_style_attribute_into_map(&value);
                    // Update inline declarations
                    self.inline_decls_by_node.insert(node, decls.clone());
                    // Extract custom properties for var() resolution later
                    let custom_props = extract_custom_properties(&decls);
                    self.inline_custom_props_by_node.insert(node, custom_props);
                    // Mark as dirty for recompute
                    self.style_changed = true;
                    self.changed_nodes.push(node);
                    return;
                }
                // Other attribute updates still mark styles dirty (class, etc.)
                self.style_changed = true;
            }
            InsertElement { .. } | InsertText { .. } | RemoveNode { .. } | EndOfDocument => {
                // For now, any structural change marks styles dirty.
                self.style_changed = true;
            }
        }
    }
}
