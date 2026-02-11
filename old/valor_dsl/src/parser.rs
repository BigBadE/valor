//! Simplified HTML parser that converts HTML strings to `DOMUpdate` messages
//!
//! This is a minimal implementation that uses html5ever for parsing but
//! converts the tree into `DOMUpdates` during construction.

use anyhow::Result;
use html5ever::tendril::TendrilSink as _;
use html5ever::{ParseOpts, parse_document};
use js::{DOMUpdate, NodeKey, NodeKeyManager};
use std::collections::HashMap;

type AttributeMap = HashMap<NodeKey, Vec<(String, String)>>;

pub struct ParseResult {
    pub updates: Vec<DOMUpdate>,
    pub attributes: AttributeMap,
}

struct ParserState<'parser_state> {
    updates: &'parser_state mut Vec<DOMUpdate>,
    attributes: &'parser_state mut AttributeMap,
    position_map: &'parser_state mut HashMap<NodeKey, usize>,
    key_manager: &'parser_state mut NodeKeyManager<usize>,
    next_id: &'parser_state mut usize,
}

use html5ever::tree_builder::TreeBuilderOpts;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

fn walk_tree(handle: &Handle, parent_key: NodeKey, state: &mut ParserState<'_>) {
    match &handle.data {
        NodeData::Element { name, attrs, .. } => {
            let id = *state.next_id;
            *state.next_id += 1;
            let node_key = state.key_manager.key_of(id);

            let tag = name.local.to_string();
            let pos = state.position_map.get(&parent_key).copied().unwrap_or(0);
            *state.position_map.entry(parent_key).or_insert(0) += 1;

            state.updates.push(DOMUpdate::InsertElement {
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

            for (attr_name, attr_value) in &attrs_vec {
                state.updates.push(DOMUpdate::SetAttr {
                    node: node_key,
                    name: String::clone(attr_name),
                    value: String::clone(attr_value),
                });
            }

            state.attributes.insert(node_key, attrs_vec);

            for child in handle.children.borrow().iter() {
                walk_tree(child, node_key, state);
            }
        }
        NodeData::Text { contents } => {
            let text = contents.borrow().to_string();
            if !text.trim().is_empty() {
                let id = *state.next_id;
                *state.next_id += 1;
                let node_key = state.key_manager.key_of(id);

                let pos = state.position_map.get(&parent_key).copied().unwrap_or(0);
                *state.position_map.entry(parent_key).or_insert(0) += 1;

                state.updates.push(DOMUpdate::InsertText {
                    parent: parent_key,
                    node: node_key,
                    text,
                    pos,
                });
            }
        }
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                walk_tree(child, parent_key, state);
            }
        }
        _ => {
            // Ignore comments, doctypes, etc.
        }
    }
}

/// Parse HTML string into `DOMUpdate` messages
///
/// # Errors
/// Returns error if parsing fails
pub fn parse_html_to_updates(
    html: &str,
    parent: NodeKey,
    key_manager: &mut NodeKeyManager<usize>,
    next_id: &mut usize,
) -> Result<ParseResult> {
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

    walk_tree(
        &dom.document,
        parent,
        &mut ParserState {
            updates: &mut updates,
            attributes: &mut attributes,
            position_map: &mut position_map,
            key_manager,
            next_id,
        },
    );

    Ok(ParseResult {
        updates,
        attributes,
    })
}
