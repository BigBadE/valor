use std::collections::HashSet;
use anyhow::Error;
use css::parser::parse_declarations;
use html::dom::updating::{DOMSubscriber, DOMUpdate};
use crate::{parse_edges_shorthand, parse_px, parse_size_spec, Display, Edges, NodeInfo, SizeSpecified, StyleEngine};

impl DOMSubscriber for StyleEngine {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        use DOMUpdate::*;
        match update {
            InsertElement {
                parent,
                node,
                tag,
                pos: _,
            } => {
                // Merge with any pending inline info that may have arrived via SetAttr before InsertElement
                let pending = self.nodes.get(&node).cloned();
                let info = NodeInfo {
                    tag: tag.clone(),
                    id: pending.as_ref().and_then(|p| p.id.clone()),
                    classes: pending
                        .as_ref()
                        .map(|p| p.classes.clone())
                        .unwrap_or_default(),
                    parent: Some(parent),
                    children: pending
                        .as_ref()
                        .map(|p| p.children.clone())
                        .unwrap_or_default(),
                    inline_display: pending.as_ref().and_then(|p| p.inline_display),
                    inline_width: pending.as_ref().and_then(|p| p.inline_width),
                    inline_height: pending.as_ref().and_then(|p| p.inline_height),
                    inline_margin: pending.as_ref().and_then(|p| p.inline_margin),
                    inline_padding: pending.as_ref().and_then(|p| p.inline_padding),
                };
                let cs = StyleEngine::compute_for_info(&info);
                self.nodes.insert(node, info);
                // link parent→child
                if let Some(pinfo) = self.nodes.get_mut(&parent) {
                    if !pinfo.children.contains(&node) {
                        pinfo.children.push(node);
                    }
                } else {
                    self.nodes.insert(
                        parent,
                        NodeInfo {
                            tag: String::new(),
                            id: None,
                            classes: HashSet::new(),
                            parent: None,
                            children: vec![node],
                            inline_display: None,
                            inline_width: None,
                            inline_height: None,
                            inline_margin: None,
                            inline_padding: None,
                        },
                    );
                }
                self.computed.insert(node, cs);
                self.add_tag_index(node, &tag);
                // Phase 2: recompute selector matches for this node
                self.rematch_node(node);
            }
            InsertText { .. } => {
                // No computed style for text nodes at the moment.
            }
            SetAttr { node, name, value } => {
                // Track inline style overrides (display, width, height, margin/padding)
                if name.eq_ignore_ascii_case("style") {
                    let parsed = parse_declarations(&value);
                    // Start from existing or new placeholder info
                    let mut info = if let Some(existing) = self.nodes.get(&node).cloned() {
                        existing
                    } else {
                        NodeInfo {
                            tag: String::new(),
                            id: None,
                            classes: HashSet::new(),
                            parent: None,
                            children: Vec::new(),
                            inline_display: None,
                            inline_width: None,
                            inline_height: None,
                            inline_margin: None,
                            inline_padding: None,
                        }
                    };
                    let mut inline_display: Option<Display> = info.inline_display;
                    let mut inline_width: Option<SizeSpecified> = info.inline_width;
                    let mut inline_height: Option<SizeSpecified> = info.inline_height;
                    let mut margin: Edges = info.inline_margin.unwrap_or_default();
                    let mut have_margin = info.inline_margin.is_some();
                    let mut padding: Edges = info.inline_padding.unwrap_or_default();
                    let mut have_padding = info.inline_padding.is_some();
                    for d in parsed {
                        let prop = d.name.to_ascii_lowercase();
                        let val = d.value.trim();
                        match prop.as_str() {
                            "display" => {
                                let v = val.to_ascii_lowercase();
                                inline_display = match v.as_str() {
                                    "none" => Some(Display::None),
                                    "block" => Some(Display::Block),
                                    "inline" => Some(Display::Inline),
                                    _ => inline_display,
                                };
                            }
                            "width" => {
                                let v = val.to_ascii_lowercase();
                                inline_width = parse_size_spec(&v).or(inline_width);
                            }
                            "height" => {
                                let v = val.to_ascii_lowercase();
                                inline_height = parse_size_spec(&v).or(inline_height);
                            }
                            "margin" => {
                                if let Some(e) = parse_edges_shorthand(val) {
                                    margin = e;
                                    have_margin = true;
                                }
                            }
                            "padding" => {
                                if let Some(e) = parse_edges_shorthand(val) {
                                    padding = e;
                                    have_padding = true;
                                }
                            }
                            "margin-top" => {
                                if let Some(px) = parse_px(val) {
                                    margin.top = px;
                                    have_margin = true;
                                }
                            }
                            "margin-right" => {
                                if let Some(px) = parse_px(val) {
                                    margin.right = px;
                                    have_margin = true;
                                }
                            }
                            "margin-bottom" => {
                                if let Some(px) = parse_px(val) {
                                    margin.bottom = px;
                                    have_margin = true;
                                }
                            }
                            "margin-left" => {
                                if let Some(px) = parse_px(val) {
                                    margin.left = px;
                                    have_margin = true;
                                }
                            }
                            "padding-top" => {
                                if let Some(px) = parse_px(val) {
                                    padding.top = px;
                                    have_padding = true;
                                }
                            }
                            "padding-right" => {
                                if let Some(px) = parse_px(val) {
                                    padding.right = px;
                                    have_padding = true;
                                }
                            }
                            "padding-bottom" => {
                                if let Some(px) = parse_px(val) {
                                    padding.bottom = px;
                                    have_padding = true;
                                }
                            }
                            "padding-left" => {
                                if let Some(px) = parse_px(val) {
                                    padding.left = px;
                                    have_padding = true;
                                }
                            }
                            _ => {}
                        }
                    }
                    info.inline_display = inline_display;
                    info.inline_width = inline_width;
                    info.inline_height = inline_height;
                    if have_margin {
                        info.inline_margin = Some(margin);
                    }
                    if have_padding {
                        info.inline_padding = Some(padding);
                    }
                    // Store back and compute (may be overwritten on InsertElement when tag is known)
                    let cs = StyleEngine::compute_for_info(&info);
                    self.nodes.insert(node, info);
                    self.computed.insert(node, cs);
                } else if name.eq_ignore_ascii_case("id") {
                    let mut info = if let Some(existing) = self.nodes.get(&node).cloned() {
                        existing
                    } else {
                        NodeInfo {
                            tag: String::new(),
                            id: None,
                            classes: HashSet::new(),
                            parent: None,
                            children: Vec::new(),
                            inline_display: None,
                            inline_width: None,
                            inline_height: None,
                            inline_margin: None,
                            inline_padding: None,
                        }
                    };
                    let old = info.id.clone();
                    let new_id = if value.is_empty() {
                        None
                    } else {
                        Some(value.clone())
                    };
                    info.id = new_id.clone();
                    self.nodes.insert(node, info);
                    self.update_id_index(node, old, new_id);
                    self.rematch_node(node);
                } else if name.eq_ignore_ascii_case("class") {
                    let mut info = if let Some(existing) = self.nodes.get(&node).cloned() {
                        existing
                    } else {
                        NodeInfo {
                            tag: String::new(),
                            id: None,
                            classes: HashSet::new(),
                            parent: None,
                            children: Vec::new(),
                            inline_display: None,
                            inline_width: None,
                            inline_height: None,
                            inline_margin: None,
                            inline_padding: None,
                        }
                    };
                    let old = info.classes.clone();
                    let new: HashSet<String> = value
                        .split_whitespace()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    info.classes = new.clone();
                    self.nodes.insert(node, info);
                    self.update_class_index(node, &old, &new);
                    self.rematch_node(node);
                }
            }
            RemoveNode { node } => {
                self.remove_node_recursive(node);
            }
            EndOfDocument => {
                // No-op for now; future work: finalize and broadcast updates
            }
        }
        Ok(())
    }
}