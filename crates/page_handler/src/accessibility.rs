//! Accessibility tree serialization for Valor browser engine.
//!
//! This module converts layout snapshots into a simple JSON accessibility tree format
//! for testing and inspection. It maps layout nodes to ARIA roles and extracts accessible
//! names from attributes or text content.

use crate::snapshots::LayoutNodeKind;
use crate::snapshots::Snapshot;
use core::hash::BuildHasher;
use css::layout_helpers::collapse_whitespace;
use js::NodeKey;
use std::collections::HashMap;

/// Type alias for a nested `HashMap` with custom hashers for element attributes.
type AttrMap<Hasher1, Hasher2> = HashMap<NodeKey, HashMap<String, String, Hasher2>, Hasher1>;

/// Escape JSON string by replacing backslashes and quotes with their escaped equivalents.
///
/// # Arguments
///
/// * `input` - The string to escape for safe JSON embedding
///
/// # Returns
///
/// A new string with backslashes and double quotes escaped
fn escape_json(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Determine the ARIA role for a layout node based on its kind and attributes.
///
/// # Arguments
///
/// * `kind` - The layout node kind to inspect
/// * `attrs` - The element's attributes for explicit role or semantic tag mapping
///
/// # Returns
///
/// A static string representing the ARIA role
fn role_for<Hasher: BuildHasher>(
    kind: &LayoutNodeKind,
    attrs: &HashMap<String, String, Hasher>,
) -> &'static str {
    match kind {
        LayoutNodeKind::Document => "document",
        LayoutNodeKind::InlineText { .. } => "text",
        LayoutNodeKind::Block { tag } => {
            if let Some(role) = attrs.get("role") {
                return Box::leak(role.clone().into_boxed_str());
            }
            match tag.to_ascii_lowercase().as_str() {
                "a" => "link",
                "button" => "button",
                "img" => "img",
                "input" | "textarea" => "textbox",
                "ul" | "ol" => "list",
                "li" => "listitem",
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
                _ => "generic",
            }
        }
    }
}

/// Extract the accessible name for a node from attributes or text content.
///
/// # Arguments
///
/// * `kind` - The layout node kind
/// * `key` - The node's key
/// * `attrs_map` - Map of all node attributes by key
///
/// # Returns
///
/// The accessible name string (may be empty)
fn name_for<Hasher1, Hasher2>(
    kind: &LayoutNodeKind,
    key: NodeKey,
    attrs_map: &AttrMap<Hasher1, Hasher2>,
) -> String
where
    Hasher1: BuildHasher,
    Hasher2: BuildHasher,
{
    if let Some(attrs) = attrs_map.get(&key) {
        if let Some(val) = attrs.get("aria-label") {
            return val.clone();
        }
        if let Some(val) = attrs.get("alt") {
            return val.clone();
        }
    }
    match kind {
        LayoutNodeKind::InlineText { text } => collapse_whitespace(text),
        _ => String::new(),
    }
}

/// Serialize a node and its subtree into JSON accessibility tree format.
///
/// # Arguments
///
/// * `node` - The node key to serialize
/// * `kind_by_key` - Map of node kinds by key
/// * `children_by_key` - Map of child keys by parent key
/// * `attrs_map` - Map of attributes by node key
///
/// # Returns
///
/// A JSON string representing the accessibility tree rooted at this node
fn serialize<Hasher1, Hasher2>(
    node: NodeKey,
    kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
    attrs_map: &AttrMap<Hasher1, Hasher2>,
) -> String
where
    Hasher1: BuildHasher,
    Hasher2: BuildHasher,
{
    let mut out = String::new();
    let kind = kind_by_key
        .get(&node)
        .cloned()
        .unwrap_or(LayoutNodeKind::Document);
    let role = attrs_map.get(&node).map_or_else(
        || {
            let empty_attrs: HashMap<String, String> = HashMap::new();
            role_for(&kind, &empty_attrs)
        },
        |attrs| role_for(&kind, attrs),
    );
    let name = escape_json(&name_for(&kind, node, attrs_map));
    out.push_str("{\"role\":\"");
    out.push_str(role);
    out.push('"');
    if !name.is_empty() {
        out.push_str(",\"name\":\"");
        out.push_str(&name);
        out.push('"');
    }
    if let Some(children) = children_by_key.get(&node)
        && !children.is_empty()
    {
        out.push_str(",\"children\":[");
        let mut first = true;
        for child in children {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&serialize(*child, kind_by_key, children_by_key, attrs_map));
        }
        out.push(']');
    }
    out.push('}');
    out
}

/// Build an accessibility tree JSON snapshot from a layout snapshot and attributes.
///
/// This function converts the layout tree into a simple JSON structure with ARIA-like
/// roles and accessible names for testing and inspection purposes.
///
/// # Arguments
///
/// * `snapshot` - The layout snapshot containing node kinds and children
/// * `attrs_map` - Map of element attributes by node key
///
/// # Returns
///
/// A JSON string representing the complete accessibility tree
pub fn ax_tree_snapshot_from<Hasher1, Hasher2>(
    snapshot: Snapshot,
    attrs_map: &AttrMap<Hasher1, Hasher2>,
) -> String
where
    Hasher1: BuildHasher,
    Hasher2: BuildHasher,
{
    let mut kind_by_key: HashMap<NodeKey, LayoutNodeKind> = HashMap::new();
    let mut children_by_key: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for (key, kind, children) in snapshot {
        kind_by_key.insert(key, kind);
        children_by_key.insert(key, children);
    }

    serialize(NodeKey::ROOT, &kind_by_key, &children_by_key, attrs_map)
}
