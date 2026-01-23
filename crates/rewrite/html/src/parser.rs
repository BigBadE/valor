//! HTML5 parsing using html5ever.

use crate::tree::TreeBuilder;
use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData as RcNodeData, RcDom};
use rewrite_core::NodeId;

/// HTML parser.
pub struct HtmlParser {
    tree_builder: TreeBuilder,
}

impl HtmlParser {
    /// Create a new HTML parser with the given tree builder.
    pub fn new(tree_builder: TreeBuilder) -> Self {
        Self { tree_builder }
    }

    /// Parse HTML from a string and build the DOM tree.
    pub fn parse(&mut self, html: &str) -> NodeId {
        // Parse with html5ever
        let opts = ParseOpts::default();
        let dom: RcDom = parse_document(RcDom::default(), opts)
            .from_utf8()
            .read_from(&mut html.as_bytes())
            .unwrap();

        // Convert html5ever's DOM to our Database representation
        self.convert_node(&dom.document, self.tree_builder.root())
    }

    /// Convert an html5ever node to our Database representation.
    fn convert_node(&mut self, rc_node: &Handle, parent: NodeId) -> NodeId {
        match &rc_node.data {
            RcNodeData::Document => {
                // For document node, process children into the root
                for child in rc_node.children.borrow().iter() {
                    self.convert_node(child, parent);
                }
                parent
            }

            RcNodeData::Doctype { .. } => {
                // Skip doctype nodes
                parent
            }

            RcNodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                // Skip empty or whitespace-only text nodes
                if text.trim().is_empty() {
                    return parent;
                }
                let node = self.tree_builder.create_text(text);
                self.tree_builder.append_child(parent, node);
                node
            }

            RcNodeData::Comment { contents } => {
                let comment = contents.to_string();
                let node = self.tree_builder.create_comment(comment);
                self.tree_builder.append_child(parent, node);
                node
            }

            RcNodeData::Element { name, attrs, .. } => {
                // Create element node
                let tag_name = name.local.to_string();
                let node = self.tree_builder.create_element(tag_name);

                // Set attributes
                for attr in attrs.borrow().iter() {
                    let attr_name = attr.name.local.to_string();
                    let attr_value = attr.value.to_string();
                    self.tree_builder.set_attribute(node, attr_name, attr_value);
                }

                // Append to parent
                self.tree_builder.append_child(parent, node);

                // Process children
                for child in rc_node.children.borrow().iter() {
                    self.convert_node(child, node);
                }

                node
            }

            RcNodeData::ProcessingInstruction { .. } => {
                // Skip processing instructions
                parent
            }
        }
    }

    /// Get the tree builder.
    pub fn tree_builder(&self) -> &TreeBuilder {
        &self.tree_builder
    }

    /// Consume the parser and return the tree builder.
    pub fn into_tree_builder(self) -> TreeBuilder {
        self.tree_builder
    }
}

/// Convenience function to parse HTML into a Database.
pub fn parse_html(html: &str) -> (rewrite_core::Database, NodeId) {
    let db = rewrite_core::Database::new();
    let tree_builder = TreeBuilder::new(db);
    let mut parser = HtmlParser::new(tree_builder);
    let root = parser.parse(html);
    let db = parser.into_tree_builder().into_database();
    (db, root)
}
