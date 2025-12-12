//! `ComputedStyle` interning for memory efficiency.
//!
//! Many nodes have identical computed styles. By interning styles, we:
//! - Save memory (one style shared by many nodes)
//! - Fast equality checks (pointer comparison)
//! - Efficient invalidation (nodes sharing same style handle invalidated together)

use css::style_types::ComputedStyle;
use js::NodeKey;
use std::collections::HashMap;
use std::hash::{Hash, Hasher as _};
use std::sync::Arc;

/// Handle to an interned `ComputedStyle`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleHandle(u32);

impl StyleHandle {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

use std::collections::hash_map::DefaultHasher;

/// Hash a `ComputedStyle` for interning.
fn hash_style(style: &ComputedStyle) -> u64 {
    let mut hasher = DefaultHasher::new();

    // Hash relevant properties for layout
    // This is a simplified version - real implementation would hash all properties
    format!("{style:?}").hash(&mut hasher);

    hasher.finish()
}

/// Interns `ComputedStyle` instances for memory efficiency.
#[derive(Debug, Default)]
pub struct StyleInterner {
    /// All interned styles
    styles: Vec<Arc<ComputedStyle>>,

    /// Hash -> handle mapping for deduplication
    hash_to_handle: HashMap<u64, StyleHandle>,

    /// Node -> style handle mapping
    node_styles: HashMap<NodeKey, StyleHandle>,
}

impl StyleInterner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a computed style and return its handle
    pub fn intern(&mut self, style: ComputedStyle) -> StyleHandle {
        let hash = hash_style(&style);

        // Check if we already have this style
        if let Some(&handle) = self.hash_to_handle.get(&hash) {
            return handle;
        }

        // New style - intern it
        let handle = StyleHandle(self.styles.len() as u32);
        self.styles.push(Arc::new(style));
        self.hash_to_handle.insert(hash, handle);
        handle
    }

    /// Get the `ComputedStyle` for a handle.
    pub fn get(&self, handle: StyleHandle) -> Option<&Arc<ComputedStyle>> {
        self.styles.get(handle.index())
    }

    /// Set the style for a node
    pub fn set_node_style(&mut self, node: NodeKey, style: ComputedStyle) -> StyleHandle {
        let handle = self.intern(style);
        self.node_styles.insert(node, handle);
        handle
    }

    /// Get the style handle for a node
    pub fn get_node_style(&self, node: NodeKey) -> Option<StyleHandle> {
        self.node_styles.get(&node).copied()
    }

    /// Remove a node's style mapping
    pub fn remove_node(&mut self, node: NodeKey) {
        self.node_styles.remove(&node);
    }

    /// Get all nodes that share a style handle
    pub fn get_nodes_with_style(&self, handle: StyleHandle) -> Vec<NodeKey> {
        self.node_styles
            .iter()
            .filter(|(_, style_handle)| **style_handle == handle)
            .map(|(node, _)| *node)
            .collect()
    }

    /// Iterate over all node->handle mappings
    pub fn node_styles_iter(&self) -> impl Iterator<Item = (&NodeKey, &StyleHandle)> {
        self.node_styles.iter()
    }

    /// Memory usage statistics
    pub fn stats(&self) -> StyleInternerStats {
        StyleInternerStats {
            unique_styles: self.styles.len(),
            total_nodes: self.node_styles.len(),
            memory_saved_bytes: self.estimate_memory_saved(),
        }
    }

    fn estimate_memory_saved(&self) -> usize {
        // Rough estimate: each ComputedStyle is ~400 bytes
        // If we have 1000 nodes and 50 unique styles, we save 950 * 400 = 380KB
        let style_size = 400;
        let without_interning = self.node_styles.len() * style_size;
        let with_interning = self.styles.len() * style_size;
        without_interning.saturating_sub(with_interning)
    }
}

/// Statistics about style interning
#[derive(Debug, Clone)]
pub struct StyleInternerStats {
    pub unique_styles: usize,
    pub total_nodes: usize,
    pub memory_saved_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test style interning.
    ///
    /// # Panics
    /// Panics if interning doesn't work correctly.
    #[test]
    fn test_style_interning() {
        let mut interner = StyleInterner::new();

        // Create two identical styles
        let style1 = ComputedStyle::default();
        let style2 = ComputedStyle::default();

        let handle1 = interner.intern(style1);
        let handle2 = interner.intern(style2);

        // Should get same handle for identical styles
        assert_eq!(handle1, handle2);
        assert_eq!(interner.styles.len(), 1);
    }

    /// Test node style mapping.
    ///
    /// # Panics
    /// Panics if node style mapping doesn't work correctly.
    #[test]
    fn test_node_style_mapping() {
        let mut interner = StyleInterner::new();

        let first_node = NodeKey::ROOT;
        let second_node = NodeKey::ROOT;

        let style = ComputedStyle::default();

        let first_handle = interner.set_node_style(first_node, style.clone());
        let second_handle = interner.set_node_style(second_node, style);

        // Both nodes should map to same handle
        assert_eq!(first_handle, second_handle);

        // Can find nodes with this style (but both are ROOT, so only 1)
        let node_list = interner.get_nodes_with_style(first_handle);
        assert_eq!(node_list.len(), 1);
    }
}
