use std::fmt;

use super::{DOM, DOMNode, NodeKind};
use indextree::NodeId;

use serde_json::{Map, Value, json};

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

fn node_to_json(dom: &DOM, id: NodeId) -> Value {
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
            for (k, v) in pairs.into_iter() {
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

        fn fmt_children(
            dom: &DOM,
            id: NodeId,
            f: &mut fmt::Formatter<'_>,
            depth: usize,
        ) -> fmt::Result {
            for child in id.children(&dom.dom) {
                fmt_node(dom, child, f, depth + 1)?;
            }
            Ok(())
        }

        fn fmt_node(
            dom: &DOM,
            id: NodeId,
            f: &mut fmt::Formatter<'_>,
            depth: usize,
        ) -> fmt::Result {
            let node_ref = dom
                .dom
                .get(id)
                .expect("NodeId in DOM printing should be valid");
            let DOMNode { kind, attrs } = node_ref.get();

            // Small helper to write sorted attributes
            fn write_attrs(
                f: &mut fmt::Formatter<'_>,
                attrs: &smallvec::SmallVec<(String, String), 4>,
            ) -> fmt::Result {
                if attrs.is_empty() {
                    return Ok(());
                }
                let mut pairs: Vec<(String, String)> = attrs.iter().cloned().collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                for (k, v) in pairs.into_iter() {
                    write!(f, " {}=\"{}\"", k, escape_text(&v))?;
                }
                Ok(())
            }

            match kind {
                NodeKind::Document => {
                    write_indent(f, depth)?;
                    writeln!(f, "#document")?;
                    fmt_children(dom, id, f, depth)?;
                }
                NodeKind::Element { tag } => {
                    write_indent(f, depth)?;
                    write!(f, "<{}", tag.to_lowercase())?;
                    write_attrs(f, attrs)?;
                    writeln!(f, ">")?;
                    fmt_children(dom, id, f, depth)?;
                    write_indent(f, depth)?;
                    writeln!(f, "</{}>", tag.to_lowercase())?;
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
