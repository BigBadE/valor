use js::NodeKey;
use layouter::LayoutNodeKind;
use std::collections::HashMap;

pub fn ax_tree_snapshot_from(
    snapshot: Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)>,
    attrs_map: HashMap<NodeKey, HashMap<String, String>>,
) -> String {
    let mut kind_by_key: HashMap<NodeKey, LayoutNodeKind> = HashMap::new();
    let mut children_by_key: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for (key, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(key, kind);
        children_by_key.insert(key, children);
    }

    fn escape_json(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }
    fn role_for(kind: &LayoutNodeKind, attrs: &HashMap<String, String>) -> &'static str {
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
                    "input" => "textbox",
                    "textarea" => "textbox",
                    "ul" | "ol" => "list",
                    "li" => "listitem",
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
                    _ => "generic",
                }
            }
        }
    }
    fn name_for(
        kind: &LayoutNodeKind,
        key: NodeKey,
        attrs_map: &HashMap<NodeKey, HashMap<String, String>>,
    ) -> String {
        if let Some(attrs) = attrs_map.get(&key) {
            if let Some(v) = attrs.get("aria-label") {
                return v.clone();
            }
            if let Some(v) = attrs.get("alt") {
                return v.clone();
            }
        }
        match kind {
            LayoutNodeKind::InlineText { text } => layouter::layout::collapse_whitespace(text),
            _ => String::new(),
        }
    }
    fn serialize(
        node: NodeKey,
        kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
        children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
        attrs_map: &HashMap<NodeKey, HashMap<String, String>>,
    ) -> String {
        let mut out = String::new();
        let kind = kind_by_key
            .get(&node)
            .cloned()
            .unwrap_or(LayoutNodeKind::Document);
        let attrs = attrs_map.get(&node).cloned().unwrap_or_default();
        let role = role_for(&kind, &attrs);
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
    serialize(NodeKey::ROOT, &kind_by_key, &children_by_key, &attrs_map)
}
