//! CSS orchestrator and mirror types exposed to other crates.
use anyhow::Result;
use js::DOMUpdate;
use std::collections::HashMap;
use url::Url;

pub use js::NodeKey;

// Bring core types into scope to avoid fully qualified paths and satisfy clippy
use css_core::CoreEngine;
use css_core::layout_model::{LayoutNodeKind as CoreLayoutNodeKind, LayoutRect as CoreLayoutRect};
use css_core::style_model::{
    BorderStyle as CoreBorderStyle, ComputedStyle as CoreComputedStyle, Overflow as CoreOverflow,
    Position as CorePosition,
};
use css_core::types::{Origin as CoreOrigin, Stylesheet as CoreStylesheet};

pub mod types {
    #[derive(Clone, Copy, Debug)]
    pub enum Origin {
        UserAgent,
        User,
        Author,
    }

    #[derive(Clone, Debug)]
    pub struct Stylesheet {
        pub rules: Vec<Rule>,
        pub origin: Origin,
    }
    impl Stylesheet {
        #[inline]
        pub const fn with_origin(origin: Origin) -> Self {
            Self {
                rules: Vec::new(),
                origin,
            }
        }
    }
    impl Default for Stylesheet {
        #[inline]
        fn default() -> Self {
            Self::with_origin(Origin::Author)
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
        /// Origin of the stylesheet rules (UA/User/Author).
        origin: Origin,
        /// Base source index for emitted rules.
        base_rule_idx: u32,
        /// Accumulated CSS text buffer.
        buf: String,
    }
    impl StylesheetStreamParser {
        #[inline]
        pub const fn new(origin: Origin, base_rule_idx: u32) -> Self {
            Self {
                origin,
                base_rule_idx,
                buf: String::new(),
            }
        }
        #[inline]
        pub fn push_chunk(&mut self, text: &str, _accum: &mut Stylesheet) {
            self.buf.push_str(text);
        }
        #[inline]
        pub fn finish_with_next(self) -> (Stylesheet, Self) {
            let sheet = Stylesheet::with_origin(self.origin);
            let next = Self::new(self.origin, self.base_rule_idx);
            (sheet, next)
        }
    }

    #[inline]
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
    #[inline]
    pub fn collapse_whitespace(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut in_ws = false;
        for character in text.chars() {
            if character.is_whitespace() {
                if !in_ws {
                    out.push(' ');
                    in_ws = true;
                }
            } else {
                in_ws = false;
                out.push(character);
            }
        }
        out.trim().to_owned()
    }
    #[inline]
    pub fn reorder_bidi_for_display(text: &str) -> String {
        text.to_owned()
    }
}

/// Snapshot of layout nodes and their child ordering for inspection.
type LayoutSnapshot = Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)>;
/// Snapshot type from the core layout engine used for mapping into public structures.
type CoreLayoutSnapshot = Vec<(NodeKey, CoreLayoutNodeKind, Vec<NodeKey>)>;

pub struct CSSMirror {
    /// Base URL used for resolving discovered stylesheet links.
    _base: Option<Url>,
    /// Aggregated parsed stylesheet from in-document <style> nodes.
    styles: types::Stylesheet,
    /// Absolute URLs of discovered external stylesheets.
    discovered: Vec<String>,
    /// Track discovered <style> nodes and their text content in insertion order
    style_nodes_order: Vec<NodeKey>,
    /// Map from style node key to its accumulated text content.
    style_text_by_node: HashMap<NodeKey, String>,
}
impl Default for CSSMirror {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl CSSMirror {
    #[inline]
    pub fn new() -> Self {
        // Avoid unwrap by deferring base initialization.
        Self {
            _base: None,
            styles: types::Stylesheet::default(),
            discovered: Vec::new(),
            style_nodes_order: Vec::new(),
            style_text_by_node: HashMap::new(),
        }
    }
    #[inline]
    pub fn with_base(url: Url) -> Self {
        Self {
            _base: Some(url),
            styles: types::Stylesheet::default(),
            discovered: Vec::new(),
            style_nodes_order: Vec::new(),
            style_text_by_node: HashMap::new(),
        }
    }
    /// Mutable reference to the aggregated in-document stylesheet.
    #[inline]
    pub const fn styles(&mut self) -> &mut types::Stylesheet {
        &mut self.styles
    }

    #[inline]
    pub fn discovered_stylesheets(&self) -> Vec<String> {
        self.discovered.clone()
    }

    /// Rebuild the aggregated stylesheet from tracked <style> nodes in DOM order.
    fn rebuild_styles_from_style_nodes(&mut self) {
        let mut out = types::Stylesheet::default();
        let mut base: u32 = 0;
        for node in &self.style_nodes_order {
            if let Some(text) = self.style_text_by_node.get(node) {
                let parsed = parser::parse_stylesheet(text, out.origin, base);
                // Avoid truncation on 64-bit by saturating len to u32::MAX
                let addend = u32::try_from(parsed.rules.len()).map_or(u32::MAX, |n| n);
                base = base.saturating_add(addend);
                out.rules.extend(parsed.rules);
            }
        }
        self.styles = out;
    }
}
use js::DOMSubscriber;
impl DOMSubscriber for CSSMirror {
    #[inline]
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        use DOMUpdate::{EndOfDocument, InsertElement, InsertText, RemoveNode, SetAttr};
        match update {
            InsertElement {
                parent: _parent,
                node,
                tag,
                ..
            } => {
                if tag.eq_ignore_ascii_case("style") && !self.style_text_by_node.contains_key(&node)
                {
                    self.style_nodes_order.push(node);
                    self.style_text_by_node.insert(node, String::new());
                }
            }
            InsertText { parent, text, .. } => {
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
            SetAttr { .. } => {}
        }
        Ok(())
    }
}

pub struct Orchestrator {
    /// Core CSS engine that performs style and layout computation.
    core: CoreEngine,
}

pub struct ProcessArtifacts {
    pub styles_changed: bool,
    pub computed_styles: HashMap<NodeKey, ComputedStyle>,
    pub layout_snapshot: LayoutSnapshot,
    pub rects: HashMap<NodeKey, LayoutRect>,
    pub dirty_rects: Vec<LayoutRect>,
}
impl Default for Orchestrator {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Orchestrator {
    #[inline]
    /// Map core computed styles to public `ComputedStyle` deterministically.
    fn map_computed(
        core_computed: HashMap<NodeKey, CoreComputedStyle>,
    ) -> HashMap<NodeKey, ComputedStyle> {
        let mut out: HashMap<NodeKey, ComputedStyle> = HashMap::new();
        let mut pairs: Vec<(NodeKey, CoreComputedStyle)> = core_computed.into_iter().collect();
        pairs.sort_by_key(|&(key, _)| key.0);
        for (key, value) in pairs {
            out.insert(
                key,
                ComputedStyle {
                    color: Rgba {
                        red: value.color.red,
                        green: value.color.green,
                        blue: value.color.blue,
                        alpha: value.color.alpha,
                    },
                    background_color: Rgba {
                        red: value.background_color.red,
                        green: value.background_color.green,
                        blue: value.background_color.blue,
                        alpha: value.background_color.alpha,
                    },
                    border_width: BorderWidths {
                        top: value.border_width.top,
                        right: value.border_width.right,
                        bottom: value.border_width.bottom,
                        left: value.border_width.left,
                    },
                    border_style: match value.border_style {
                        CoreBorderStyle::None => BorderStyle::None,
                        CoreBorderStyle::Solid => BorderStyle::Solid,
                    },
                    border_color: Rgba {
                        red: value.border_color.red,
                        green: value.border_color.green,
                        blue: value.border_color.blue,
                        alpha: value.border_color.alpha,
                    },
                    font_size: value.font_size,
                    overflow: match value.overflow {
                        CoreOverflow::Visible => Overflow::Visible,
                        CoreOverflow::Hidden => Overflow::Hidden,
                    },
                    position: match value.position {
                        CorePosition::Static => Position::Static,
                        CorePosition::Relative => Position::Relative,
                        CorePosition::Absolute => Position::Absolute,
                        CorePosition::Fixed => Position::Fixed,
                    },
                    z_index: value.z_index,
                },
            );
        }
        out
    }

    #[inline]
    /// Map core layout rects to public `LayoutRect` deterministically.
    fn map_rects(core_rects: HashMap<NodeKey, CoreLayoutRect>) -> HashMap<NodeKey, LayoutRect> {
        let mut out: HashMap<NodeKey, LayoutRect> = HashMap::new();
        let mut pairs: Vec<(NodeKey, CoreLayoutRect)> = core_rects.into_iter().collect();
        pairs.sort_by_key(|&(key, _)| key.0);
        for (key, rect) in pairs {
            out.insert(
                key,
                LayoutRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                },
            );
        }
        out
    }

    #[inline]
    /// Map core layout snapshot to public snapshot types.
    fn map_layout_snapshot(core_snapshot: CoreLayoutSnapshot) -> LayoutSnapshot {
        core_snapshot
            .into_iter()
            .map(|(key, kind, children)| {
                let mapped_kind = match kind {
                    CoreLayoutNodeKind::Document => LayoutNodeKind::Document,
                    CoreLayoutNodeKind::Block { tag } => LayoutNodeKind::Block { tag },
                    CoreLayoutNodeKind::InlineText { text } => LayoutNodeKind::InlineText { text },
                };
                (key, mapped_kind, children)
            })
            .collect()
    }
    #[inline]
    pub fn new() -> Self {
        Self {
            core: CoreEngine::new(),
        }
    }
    /// Apply a `DOMUpdate` to the core engine.
    ///
    /// # Errors
    /// Returns an error if the core engine reports a failure during update application.
    #[inline]
    pub fn apply_dom_update(&mut self, update: DOMUpdate) -> Result<()> {
        self.core.apply_dom_update(update)
    }
    /// Replace the current stylesheet used by the engine.
    #[inline]
    pub fn replace_stylesheet(&mut self, sheet: &types::Stylesheet) {
        // Map orchestrator public type to core type
        let core_sheet = CoreStylesheet {
            rules: Vec::new(),
            origin: match sheet.origin {
                types::Origin::UserAgent => CoreOrigin::UserAgent,
                types::Origin::User => CoreOrigin::User,
                types::Origin::Author => CoreOrigin::Author,
            },
        };
        self.core.replace_stylesheet(core_sheet);
    }
    /// Execute one processing pass and return artifacts for rendering and inspection.
    ///
    /// # Errors
    /// Returns an error if the core engine encounters a failure during processing.
    #[inline]
    pub fn process_once(&mut self) -> Result<ProcessArtifacts> {
        let styles_changed = self.core.recompute_styles();
        let core_computed = self.core.computed_snapshot();
        let core_rects = self.core.compute_layout();
        let core_dirty = self.core.take_dirty_rects();
        let core_snapshot = self.core.layout_snapshot();

        // Map core types to public orchestrator types
        let computed = Self::map_computed(core_computed);

        let rects = Self::map_rects(core_rects);

        let dirty_rects: Vec<LayoutRect> = core_dirty
            .into_iter()
            .map(|rect| LayoutRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            })
            .collect();

        let layout_snapshot: LayoutSnapshot = Self::map_layout_snapshot(core_snapshot);
        Ok(ProcessArtifacts {
            styles_changed,
            computed_styles: computed,
            layout_snapshot,
            rects,
            dirty_rects,
        })
    }
}
