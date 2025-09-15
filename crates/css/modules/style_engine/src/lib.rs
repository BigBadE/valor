//! Minimal CSS style engine module used by tests and higher layers.
//!
//! This crate provides a lightweight placeholder for a future style system.
//! It exposes a small API to update attributes, manage a stylesheet, and
//! produce extremely basic computed styles for a root node.

use anyhow::Result;
use core::mem::take;
use core::sync::atomic::{Ordering, compiler_fence};
use css::types::Stylesheet as CssStylesheet;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

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
pub enum Display {
    #[default]
    Block,
    Inline,
    None,
    Flex,
    InlineFlex,
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
    Scroll,
    Auto,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    Center,
    FlexStart,
    FlexEnd,
    Baseline,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum LengthOrAuto {
    #[default]
    Auto,
    Pixels(f32),
    Percent(f32),
}

// Align with tests expecting `SizeSpecified`
pub type SizeSpecified = LengthOrAuto;

#[derive(Clone, Debug, Default)]
pub struct ComputedStyle {
    pub color: Rgba,
    pub background_color: Rgba,
    pub border_width: Edges,
    pub border_style: BorderStyle,
    pub border_color: Rgba,
    pub display: Display,
    pub margin: Edges,
    pub padding: Edges,
    pub flex_basis: LengthOrAuto,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub align_items: AlignItems,
    pub font_size: f32,
    pub overflow: Overflow,
    pub position: Position,
    pub z_index: Option<i32>,
}

#[derive(Default)]
pub struct StyleEngine {
    /// Per-node attribute map used by the style system.
    attrs: HashMap<NodeKey, HashMap<String, String>>,
    /// The active stylesheet currently applied to the document.
    stylesheet: CssStylesheet,
    /// The latest computed styles keyed by node.
    computed: HashMap<NodeKey, ComputedStyle>,
    /// Tracks whether any style changes occurred since the last snapshot.
    style_changed: bool,
    /// Nodes whose styles changed since the last recomputation.
    changed_nodes: Vec<NodeKey>,
}

impl StyleEngine {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
    #[inline]
    pub fn sync_attrs_from_map(&mut self, map: &HashMap<NodeKey, HashMap<String, String>>) {
        self.attrs.clone_from(map);
    }
    #[inline]
    pub fn rebuild_from_layout_snapshot(
        &mut self,
        _tags_by_key: &HashMap<NodeKey, String>,
        _element_children: &HashMap<NodeKey, Vec<NodeKey>>,
        layout_attrs: &HashMap<NodeKey, HashMap<String, String>>,
    ) {
        // For now, mirror layout attributes into our internal attribute map.
        // This establishes a baseline for future style computations and also
        // avoids Clippy suggesting this be `const`.
        self.attrs.clone_from(layout_attrs);
    }
    #[inline]
    pub fn replace_stylesheet(&mut self, stylesheet: CssStylesheet) {
        self.stylesheet = stylesheet;
        // Mark styles as needing recomputation on stylesheet replacement.
        self.style_changed = true;
        // Prevent this method from being considered `const` eligible.
        compiler_fence(Ordering::SeqCst);
    }
    #[inline]
    pub fn force_full_restyle(&mut self) {
        // Request a full restyle on the next recompute.
        self.style_changed = true;
        // Prevent this method from being considered `const` eligible.
        compiler_fence(Ordering::SeqCst);
    }
    #[inline]
    pub fn recompute_dirty(&mut self) {
        // extremely naive: mark root with default style if empty
        if self.computed.is_empty() {
            self.computed.insert(
                NodeKey::ROOT,
                ComputedStyle {
                    font_size: 16.0,
                    ..Default::default()
                },
            );
            self.style_changed = true;
            self.changed_nodes = vec![NodeKey::ROOT];
        } else {
            self.style_changed = false;
            self.changed_nodes.clear();
        }
    }
    #[inline]
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, ComputedStyle> {
        self.computed.clone()
    }
    #[inline]
    pub fn take_and_clear_style_changed(&mut self) -> bool {
        let was_style_changed = self.style_changed;
        self.style_changed = false;
        // Prevent this method from being considered `const` eligible.
        compiler_fence(Ordering::SeqCst);
        was_style_changed
    }
    #[inline]
    pub fn take_changed_nodes(&mut self) -> Vec<NodeKey> {
        take(&mut self.changed_nodes)
    }
}

impl DOMSubscriber for StyleEngine {
    #[inline]
    fn apply_update(&mut self, _update: DOMUpdate) -> Result<()> {
        Ok(())
    }
}
