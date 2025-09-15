use anyhow::Result;
use js::DOMUpdate;
use std::collections::HashMap;

pub use js::NodeKey;

pub mod types {
    #[derive(Clone, Copy, Debug)]
    pub enum Origin {
        UserAgent,
        User,
        Author,
        UA,
    }

    #[derive(Clone, Debug)]
    pub struct Stylesheet {
        pub rules: Vec<Rule>,
        pub origin: Origin,
    }
    impl Stylesheet {
        pub fn with_origin(origin: Origin) -> Self {
            Self {
                rules: Vec::new(),
                origin,
            }
        }
    }
    impl Default for Stylesheet {
        fn default() -> Self {
            Stylesheet::with_origin(Origin::Author)
        }
    }

    #[derive(Clone, Debug)]
    pub struct Rule {
        pub origin: Origin,
        pub source_order: u32,
    }
}

pub mod parser {
    use super::types::Rule;
    use super::types::{Origin, Stylesheet};
    pub struct StylesheetStreamParser {
        origin: Origin,
        base_rule_idx: u32,
        buf: String,
    }
    impl StylesheetStreamParser {
        pub fn new(origin: Origin, base_rule_idx: u32) -> Self {
            Self {
                origin,
                base_rule_idx,
                buf: String::new(),
            }
        }
        pub fn push_chunk(&mut self, text: &str, _accum: &mut Stylesheet) {
            self.buf.push_str(text);
        }
        pub fn finish_with_next(self) -> (Stylesheet, StylesheetStreamParser) {
            let sheet = Stylesheet::with_origin(self.origin);
            let next = StylesheetStreamParser::new(self.origin, self.base_rule_idx);
            (sheet, next)
        }
    }

    pub fn parse_stylesheet(_css: &str, origin: Origin, base_rule_idx: u32) -> Stylesheet {
        // Minimal shim: produce exactly one rule per <style> block, with monotonic source_order
        let mut sheet = Stylesheet::with_origin(origin);
        sheet.rules.push(Rule {
            origin,
            source_order: base_rule_idx,
        });
        sheet
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct BorderWidths {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
#[derive(Clone, Copy, Debug, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
}
#[derive(Clone, Copy, Debug, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
}
#[derive(Clone, Copy, Debug, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Clone, Debug, Default)]
pub struct ComputedStyle {
    pub color: Rgba,
    pub background_color: Rgba,
    pub border_width: BorderWidths,
    pub border_style: BorderStyle,
    pub border_color: Rgba,
    pub font_size: f32,
    pub overflow: Overflow,
    pub position: Position,
    pub z_index: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    Document,
    Block { tag: String },
    InlineText { text: String },
}

pub mod layout_helpers {
    pub fn collapse_whitespace(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut in_ws = false;
        for ch in s.chars() {
            if ch.is_whitespace() {
                if !in_ws {
                    out.push(' ');
                    in_ws = true;
                }
            } else {
                in_ws = false;
                out.push(ch);
            }
        }
        out.trim().to_string()
    }
    pub fn reorder_bidi_for_display(s: &str) -> String {
        s.to_string()
    }
}

pub struct CSSMirror {
    _base: url::Url,
    styles: types::Stylesheet,
    discovered: Vec<String>,
    // Track discovered <style> nodes and their text content in insertion order
    style_nodes_order: Vec<NodeKey>,
    style_text_by_node: HashMap<NodeKey, String>,
}
impl Default for CSSMirror {
    fn default() -> Self {
        Self::new()
    }
}

impl CSSMirror {
    pub fn new() -> Self {
        Self::with_base(
            url::Url::parse("about:blank")
                .unwrap_or_else(|_| url::Url::parse("http://localhost/").unwrap()),
        )
    }
    pub fn with_base(url: url::Url) -> Self {
        Self {
            _base: url,
            styles: types::Stylesheet::default(),
            discovered: Vec::new(),
            style_nodes_order: Vec::new(),
            style_text_by_node: HashMap::new(),
        }
    }
    pub fn styles(&mut self) -> &mut types::Stylesheet {
        &mut self.styles
    }
    pub fn discovered_stylesheets(&self) -> Vec<String> {
        self.discovered.clone()
    }

    fn rebuild_styles_from_style_nodes(&mut self) {
        let mut out = types::Stylesheet::default();
        let mut base: u32 = 0;
        for node in &self.style_nodes_order {
            if let Some(text) = self.style_text_by_node.get(node) {
                let parsed = parser::parse_stylesheet(text, out.origin, base);
                base = base.saturating_add(parsed.rules.len() as u32);
                out.rules.extend(parsed.rules);
            }
        }
        self.styles = out;
    }
}
impl js::DOMSubscriber for CSSMirror {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        use DOMUpdate::*;
        match update {
            InsertElement {
                parent: _parent,
                node,
                tag,
                pos: _,
            } => {
                if tag.eq_ignore_ascii_case("style") && !self.style_text_by_node.contains_key(&node)
                {
                    self.style_nodes_order.push(node);
                    self.style_text_by_node.insert(node, String::new());
                }
            }
            InsertText {
                parent,
                node: _n,
                text,
                pos: _,
            } => {
                if self.style_text_by_node.contains_key(&parent) {
                    let entry = self.style_text_by_node.entry(parent).or_default();
                    entry.push_str(&text);
                }
            }
            RemoveNode { node } => {
                if self.style_text_by_node.remove(&node).is_some() {
                    self.style_nodes_order.retain(|n| *n != node);
                    // Retract rules for this style node immediately
                    self.rebuild_styles_from_style_nodes();
                }
            }
            EndOfDocument => {
                self.rebuild_styles_from_style_nodes();
            }
            _ => {}
        }
        Ok(())
    }
}

pub struct Orchestrator {
    core: css_core::CoreEngine,
}

pub struct ProcessArtifacts {
    pub styles_changed: bool,
    pub computed_styles: HashMap<NodeKey, ComputedStyle>,
    pub layout_snapshot: Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)>,
    pub rects: HashMap<NodeKey, LayoutRect>,
    pub dirty_rects: Vec<LayoutRect>,
}
impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            core: css_core::CoreEngine::new(),
        }
    }
    pub fn apply_dom_update(&mut self, update: js::DOMUpdate) -> Result<()> {
        self.core.apply_dom_update(update)
    }
    pub fn replace_stylesheet(&mut self, sheet: types::Stylesheet) {
        // Map orchestrator public type to core type
        let core_sheet = css_core::types::Stylesheet {
            rules: Vec::new(),
            origin: match sheet.origin {
                types::Origin::UserAgent | types::Origin::UA => css_core::types::Origin::UserAgent,
                types::Origin::User => css_core::types::Origin::User,
                types::Origin::Author => css_core::types::Origin::Author,
            },
        };
        self.core.replace_stylesheet(core_sheet);
    }
    pub fn process_once(&mut self) -> Result<ProcessArtifacts> {
        let styles_changed = self.core.recompute_styles()?;
        let core_computed = self.core.computed_snapshot();
        let core_rects = self.core.compute_layout();
        let core_dirty = self.core.take_dirty_rects();
        let core_snapshot = self.core.layout_snapshot();

        // Map core types to public orchestrator types
        let mut computed: HashMap<NodeKey, ComputedStyle> = HashMap::new();
        for (k, v) in core_computed.into_iter() {
            computed.insert(
                k,
                ComputedStyle {
                    color: Rgba {
                        red: v.color.red,
                        green: v.color.green,
                        blue: v.color.blue,
                        alpha: v.color.alpha,
                    },
                    background_color: Rgba {
                        red: v.background_color.red,
                        green: v.background_color.green,
                        blue: v.background_color.blue,
                        alpha: v.background_color.alpha,
                    },
                    border_width: BorderWidths {
                        top: v.border_width.top,
                        right: v.border_width.right,
                        bottom: v.border_width.bottom,
                        left: v.border_width.left,
                    },
                    border_style: match v.border_style {
                        css_core::style_model::BorderStyle::None => BorderStyle::None,
                        css_core::style_model::BorderStyle::Solid => BorderStyle::Solid,
                    },
                    border_color: Rgba {
                        red: v.border_color.red,
                        green: v.border_color.green,
                        blue: v.border_color.blue,
                        alpha: v.border_color.alpha,
                    },
                    font_size: v.font_size,
                    overflow: match v.overflow {
                        css_core::style_model::Overflow::Visible => Overflow::Visible,
                        css_core::style_model::Overflow::Hidden => Overflow::Hidden,
                    },
                    position: match v.position {
                        css_core::style_model::Position::Static => Position::Static,
                        css_core::style_model::Position::Relative => Position::Relative,
                        css_core::style_model::Position::Absolute => Position::Absolute,
                        css_core::style_model::Position::Fixed => Position::Fixed,
                    },
                    z_index: v.z_index,
                },
            );
        }

        let mut rects: HashMap<NodeKey, LayoutRect> = HashMap::new();
        for (k, r) in core_rects.into_iter() {
            rects.insert(
                k,
                LayoutRect {
                    x: r.x,
                    y: r.y,
                    width: r.width,
                    height: r.height,
                },
            );
        }

        let dirty_rects: Vec<LayoutRect> = core_dirty
            .into_iter()
            .map(|r| LayoutRect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            })
            .collect();

        let layout_snapshot: Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)> = core_snapshot
            .into_iter()
            .map(|(k, kind, kids)| {
                let kk = match kind {
                    css_core::layout_model::LayoutNodeKind::Document => LayoutNodeKind::Document,
                    css_core::layout_model::LayoutNodeKind::Block { tag } => {
                        LayoutNodeKind::Block { tag }
                    }
                    css_core::layout_model::LayoutNodeKind::InlineText { text } => {
                        LayoutNodeKind::InlineText { text }
                    }
                };
                (k, kk, kids)
            })
            .collect();
        Ok(ProcessArtifacts {
            styles_changed,
            computed_styles: computed,
            layout_snapshot,
            rects,
            dirty_rects,
        })
    }
}
