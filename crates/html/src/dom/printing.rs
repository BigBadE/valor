use core::fmt;

use super::{DOM, DOMNode, NodeKind};
use indextree::NodeId;
use serde_json::{Map, Value, json};
use smallvec::SmallVec;

// -----------------------
// Module-scope helpers
// -----------------------

fn flush_text(children: &mut Vec<Value>, text_buf: &mut String) {
    if !text_buf.trim().is_empty() {
        children.push(json!({ "type": "text", "text": text_buf.clone() }));
    }
    text_buf.clear();
}

fn push_non_null(children: &mut Vec<Value>, v: Value) {
    if !v.is_null() {
        children.push(v);
    }
}

fn coalesce_children(dom: &DOM, id: NodeId) -> Vec<Value> {
    let mut children: Vec<Value> = Vec::new();
    let mut text_buf = String::new();
    for c in id.children(&dom.dom) {
        let cref = dom.dom.get(c).expect("Child NodeId valid");
        if let NodeKind::Text { text } = &cref.get().kind {
            text_buf.push_str(text);
            continue;
        }
        flush_text(&mut children, &mut text_buf);
        let v = node_to_json(dom, c);
        push_non_null(&mut children, v);
    }
    flush_text(&mut children, &mut text_buf);
    children
}

pub(super) fn node_to_json(dom: &DOM, id: NodeId) -> Value {
    let node_ref = dom
        .dom
        .get(id)
        .expect("NodeId should be valid during JSON snapshot");
    let DOMNode { kind, attrs } = node_ref.get();
    match kind.clone() {
        NodeKind::Document => json!({ "type": "document", "children": coalesce_children(dom, id) }),
        NodeKind::Element { tag } => {
            // Convert attrs SmallVec to map and sort by key for determinism
            let mut pairs: Vec<(String, String)> = attrs.iter().cloned().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut attrs_obj = Map::new();
            for (k, v) in pairs {
                attrs_obj.insert(k, Value::String(v));
            }
            let children = coalesce_children(dom, id);
            json!({
                "type": "element",
                "tag": tag.to_lowercase(),
                "attrs": Value::Object(attrs_obj),
                "children": children,
            })
        }
        NodeKind::Text { text } => {
            if text.trim().is_empty() {
                Value::Null
            } else {
                json!({ "type": "text", "text": text })
            }
        }
    }
}

impl fmt::Debug for DOM {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Header
        writeln!(f, "DOM")?;

        // Pretty print starting from root
        fn write_indent(formatter: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
            for _ in 0..depth {
                formatter.write_str("  ")?;
            }
            Ok(())
        }

        fn escape_text(text: &str) -> String {
            let mut out = String::with_capacity(text.len());
            for character in text.chars() {
                match character {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    _ => out.push(character),
                }
            }
            out
        }

        fn fmt_children(
            dom: &DOM,
            id: NodeId,
            formatter: &mut fmt::Formatter<'_>,
            depth: usize,
        ) -> fmt::Result {
            for child in id.children(&dom.dom) {
                fmt_node(dom, child, formatter, depth + 1)?;
            }
            Ok(())
        }

        fn fmt_node(
            dom: &DOM,
            id: NodeId,
            formatter: &mut fmt::Formatter<'_>,
            depth: usize,
        ) -> fmt::Result {
            let Some(node_ref) = dom.dom.get(id) else {
                return write!(formatter, "<invalid-node>");
            };
            let DOMNode { kind, attrs } = node_ref.get();

            // Small helper to write sorted attributes
            fn write_attrs(
                formatter: &mut fmt::Formatter<'_>,
                attrs: &SmallVec<(String, String), 4>,
            ) -> fmt::Result {
                if attrs.is_empty() {
                    return Ok(());
                }
                let mut pairs: Vec<(String, String)> = attrs.iter().cloned().collect();
                pairs.sort_by(|attr_a, attr_b| attr_a.0.cmp(&attr_b.0));
                for (key, value) in pairs {
                    write!(formatter, " {}=\"{}\"", key, escape_text(&value))?;
                }
                Ok(())
            }

            match kind {
                NodeKind::Document => {
                    write_indent(formatter, depth)?;
                    writeln!(formatter, "#document")?;
                    fmt_children(dom, id, formatter, depth)?;
                }
                NodeKind::Element { tag } => {
                    write_indent(formatter, depth)?;
                    write!(formatter, "<{}", tag.to_lowercase())?;
                    write_attrs(formatter, attrs)?;
                    writeln!(formatter, ">")?;
                    fmt_children(dom, id, formatter, depth)?;
                    write_indent(formatter, depth)?;
                    writeln!(formatter, "</{}>", tag.to_lowercase())?;
                }
                NodeKind::Text { text } => {
                    // Skip pure-whitespace text nodes in the printer for cleaner output
                    if text.chars().all(char::is_whitespace) {
                        return Ok(());
                    }
                    write_indent(formatter, depth)?;
                    writeln!(formatter, "\"{}\"", escape_text(text))?;
                }
            }
            Ok(())
        }

        fmt_node(self, self.root, f, 0)
    }
}

// DOM printing methods moved to mod.rs to avoid multiple impl blocks
