use std::fmt;

use super::{DOMNode, NodeKind, DOM};
use indextree::NodeId;

use serde_json::{json, Map, Value};

impl fmt::Debug for DOM {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Header
        writeln!(f, "DOM")?;

        // Pretty print starting from root
        fn write_indent(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
            for _ in 0..depth {
                f.write_str("  ")?;
            }
            Ok(())
        }

        fn escape_text(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            for ch in s.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    _ => out.push(ch),
                }
            }
            out
        }

        fn fmt_node(dom: &DOM, id: NodeId, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
            let node_ref = dom
                .dom
                .get(id)
                .expect("NodeId in DOM printing should be valid");
            let DOMNode { kind, attrs } = node_ref.get();

            match kind {
                NodeKind::Document => {
                    write_indent(f, depth)?;
                    writeln!(f, "#document")?;
                    for child in id.children(&dom.dom) {
                        fmt_node(dom, child, f, depth + 1)?;
                    }
                }
                NodeKind::Element { tag } => {
                    write_indent(f, depth)?;
                    f.write_str("<")?;
                    f.write_str(tag)?;
                    if !attrs.is_empty() {
                        for (k, v) in attrs.iter() {
                            f.write_str(" ")?;
                            f.write_str(k)?;
                            f.write_str("=\"")?;
                            f.write_str(&escape_text(v))?;
                            f.write_str("\"")?;
                        }
                    }

                    // Determine if there are children
                    let children = id.children(&dom.dom);
                    if children.clone().next().is_none() {
                        // No children; render as self-contained line
                        f.write_str(">")?;
                        f.write_str("</")?;
                        f.write_str(tag)?;
                        writeln!(f, ">")?;
                    } else {
                        writeln!(f, ">")?;
                        for child in children {
                            fmt_node(dom, child, f, depth + 1)?;
                        }
                        write_indent(f, depth)?;
                        f.write_str("</")?;
                        f.write_str(tag)?;
                        writeln!(f, ">")?;
                    }
                }
                NodeKind::Text { text } => {
                    // Skip pure-whitespace text nodes in the printer for cleaner output
                    if text.chars().all(|c| c.is_whitespace()) {
                        return Ok(());
                    }
                    write_indent(f, depth)?;
                    writeln!(f, "\"{}\"", escape_text(text))?;
                }
            }
            Ok(())
        }

        fmt_node(self, self.root, f, 0)
    }
}

impl DOM {
    /// Build a deterministic JSON representation of the DOM.
    /// Schema:
    /// - Document: { "type":"document", "children":[ ... ] }
    /// - Element: { "type":"element", "tag": "div", "attrs": {..}, "children":[ ... ] }
    /// - Text: { "type":"text", "text":"..." }
    pub fn to_json_value(&self) -> Value {
        fn node_to_json(dom: &DOM, id: NodeId) -> Value {
            let node_ref = dom
                .dom
                .get(id)
                .expect("NodeId should be valid during JSON snapshot");
            let DOMNode { kind, attrs } = node_ref.get();
            match kind.clone() {
                NodeKind::Document => {
                    // Collect children, coalescing adjacent text nodes and preserving whitespace within runs
                    let mut children: Vec<Value> = Vec::new();
                    let mut text_buf = String::new();
                    for c in id.children(&dom.dom) {
                        let cref = dom.dom.get(c).expect("Child NodeId valid");
                        match &cref.get().kind {
                            NodeKind::Text { text } => {
                                text_buf.push_str(text);
                            }
                            _ => {
                                if !text_buf.trim().is_empty() {
                                    children.push(json!({ "type": "text", "text": text_buf.clone() }));
                                }
                                text_buf.clear();
                                let v = node_to_json(dom, c);
                                if !v.is_null() {
                                    children.push(v);
                                }
                            }
                        }
                    }
                    if !text_buf.trim().is_empty() {
                        children.push(json!({ "type": "text", "text": text_buf }));
                    }
                    json!({ "type": "document", "children": children })
                }
                NodeKind::Element { tag } => {
                    // Convert attrs SmallVec to map and sort by key for determinism
                    let mut pairs: Vec<(String, String)> = attrs.iter().cloned().collect();
                    pairs.sort_by(|a, b| a.0.cmp(&b.0));
                    let mut attrs_obj = Map::new();
                    for (k, v) in pairs.into_iter() {
                        attrs_obj.insert(k, Value::String(v));
                    }
                    // Collect children with text coalescing
                    let mut children: Vec<Value> = Vec::new();
                    let mut text_buf = String::new();
                    for c in id.children(&dom.dom) {
                        let cref = dom.dom.get(c).expect("Child NodeId valid");
                        match &cref.get().kind {
                            NodeKind::Text { text } => {
                                text_buf.push_str(text);
                            }
                            _ => {
                                if !text_buf.trim().is_empty() {
                                    children.push(json!({ "type": "text", "text": text_buf.clone() }));
                                }
                                text_buf.clear();
                                let v = node_to_json(dom, c);
                                if !v.is_null() {
                                    children.push(v);
                                }
                            }
                        }
                    }
                    if !text_buf.trim().is_empty() {
                        children.push(json!({ "type": "text", "text": text_buf }));
                    }
                    json!({
                        "type": "element",
                        "tag": tag.to_lowercase(),
                        "attrs": Value::Object(attrs_obj),
                        "children": children,
                    })
                }
                NodeKind::Text { text } => {
                    // For standalone text nodes (e.g., root text), keep if not pure whitespace
                    if text.trim().is_empty() { Value::Null } else { json!({ "type": "text", "text": text }) }
                }
            }
        }
        node_to_json(self, self.root)
    }

    /// Pretty JSON string for snapshots and test comparisons.
    pub fn to_json_string(&self) -> String {
        match serde_json::to_string_pretty(&self.to_json_value()) {
            Ok(s) => s,
            Err(_) => String::from("{}"),
        }
    }
}
