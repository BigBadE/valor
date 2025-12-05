//! Simplified HTML parser that converts HTML strings to DOMUpdate messages
//!
//! This is a minimal implementation that uses html5ever for parsing but
//! converts the tree into DOMUpdates during construction.

use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, ParseOpts};
use js::{DOMUpdate, NodeKey, NodeKeyManager};
use std::collections::HashMap;

pub struct ParseResult {
    pub updates: Vec<DOMUpdate>,
    pub attributes: HashMap<NodeKey, Vec<(String, String)>>,
}

/// Parse HTML string into DOMUpdate messages
///
/// # Errors
/// Returns error if parsing fails
pub fn parse_html_to_updates(
    html: &str,
    parent: NodeKey,
    key_manager: &mut NodeKeyManager<usize>,
    next_id: &mut usize,
) -> anyhow::Result<ParseResult> {
    // For now, use a simple approach: parse with rcdom and convert
    use markup5ever_rcdom::{RcDom, Handle, NodeData};
    use html5ever::tree_builder::TreeBuilderOpts;

    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            exact_errors: false,
            scripting_enabled: false,
            ..TreeBuilderOpts::default()
        },
        ..ParseOpts::default()
    };

    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())?;

    let mut updates = Vec::new();
    let mut attributes = HashMap::new();
    let mut position_map: HashMap<NodeKey, usize> = HashMap::new();

    fn walk_tree(
        handle: &Handle,
        parent_key: NodeKey,
        updates: &mut Vec<DOMUpdate>,
        attributes: &mut HashMap<NodeKey, Vec<(String, String)>>,
        position_map: &mut HashMap<NodeKey, usize>,
        key_manager: &mut NodeKeyManager<usize>,
        next_id: &mut usize,
    ) {
        match &handle.data {
            NodeData::Element { name, attrs, .. } => {
                let id = *next_id;
                *next_id += 1;
                let node_key = key_manager.key_of(id);

                let tag = name.local.to_string();
                let pos = position_map.get(&parent_key).copied().unwrap_or(0);
                *position_map.entry(parent_key).or_insert(0) += 1;

                updates.push(DOMUpdate::InsertElement {
                    parent: parent_key,
                    node: node_key,
                    tag,
                    pos,
                });

                let attrs_vec: Vec<(String, String)> = attrs
                    .borrow()
                    .iter()
                    .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                    .collect();

                for (name, value) in &attrs_vec {
                    updates.push(DOMUpdate::SetAttr {
                        node: node_key,
                        name: String::clone(name),
                        value: String::clone(value),
                    });
                }

                attributes.insert(node_key, attrs_vec);

                for child in handle.children.borrow().iter() {
                    walk_tree(child, node_key, updates, attributes, position_map, key_manager, next_id);
                }
            }
            NodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                if !text.trim().is_empty() {
                    let id = *next_id;
                    *next_id += 1;
                    let node_key = key_manager.key_of(id);

                    let pos = position_map.get(&parent_key).copied().unwrap_or(0);
                    *position_map.entry(parent_key).or_insert(0) += 1;

                    updates.push(DOMUpdate::InsertText {
                        parent: parent_key,
                        node: node_key,
                        text,
                        pos,
                    });
                }
            }
            NodeData::Document => {
                for child in handle.children.borrow().iter() {
                    walk_tree(child, parent_key, updates, attributes, position_map, key_manager, next_id);
                }
            }
            _ => {
                // Ignore comments, doctypes, etc.
            }
        }
    }

    walk_tree(
        &dom.document,
        parent,
        &mut updates,
        &mut attributes,
        &mut position_map,
        key_manager,
        next_id,
    );

    Ok(ParseResult {
        updates,
        attributes,
    })
}
