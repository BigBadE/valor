use std::fmt;

use js::NodeKey;

use crate::{LayoutNodeKind, Layouter};

impl fmt::Debug for Layouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Header
        writeln!(f, "LAYOUT")?;

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

        // Build lookup maps from the public snapshot
        let snapshot = self.snapshot();
        let mut kind_by_key = std::collections::HashMap::new();
        let mut children_by_key = std::collections::HashMap::new();
        for (key, kind, children) in snapshot.into_iter() {
            kind_by_key.insert(key, kind);
            children_by_key.insert(key, children);
        }

        fn fmt_node(
            key: NodeKey,
            kind_by_key: &std::collections::HashMap<NodeKey, LayoutNodeKind>,
            children_by_key: &std::collections::HashMap<NodeKey, Vec<NodeKey>>,
            f: &mut fmt::Formatter<'_>,
            depth: usize,
        ) -> fmt::Result {
            match kind_by_key.get(&key) {
                Some(LayoutNodeKind::Document) | None => {
                    write_indent(f, depth)?;
                    writeln!(f, "#document")?;
                    if let Some(children) = children_by_key.get(&key) {
                        for child in children {
                            fmt_node(*child, kind_by_key, children_by_key, f, depth + 1)?;
                        }
                    }
                }
                Some(LayoutNodeKind::Block { tag }) => {
                    write_indent(f, depth)?;
                    f.write_str("<")?;
                    f.write_str(tag)?;
                    if children_by_key.get(&key).map(|v| v.is_empty()).unwrap_or(true) {
                        f.write_str(">")?;
                        f.write_str("</")?;
                        f.write_str(tag)?;
                        writeln!(f, ">")?;
                    } else {
                        writeln!(f, ">")?;
                        if let Some(children) = children_by_key.get(&key) {
                            for child in children {
                                fmt_node(*child, kind_by_key, children_by_key, f, depth + 1)?;
                            }
                        }
                        write_indent(f, depth)?;
                        f.write_str("</")?;
                        f.write_str(tag)?;
                        writeln!(f, ">")?;
                    }
                }
                Some(LayoutNodeKind::InlineText { text }) => {
                    if text.chars().all(|c| c.is_whitespace()) {
                        return Ok(());
                    }
                    write_indent(f, depth)?;
                    writeln!(f, "\"{}\"", escape_text(text))?;
                }
            }
            Ok(())
        }

        fmt_node(NodeKey::ROOT, &kind_by_key, &children_by_key, f, 0)
    }
}
