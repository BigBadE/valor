use anyhow::Result;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;
use std::mem::take;

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
    Px(f32),
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

type Stylesheet = css::types::Stylesheet;

#[derive(Default)]
pub struct StyleEngine {
    attrs: HashMap<NodeKey, HashMap<String, String>>,
    stylesheet: Stylesheet,
    computed: HashMap<NodeKey, ComputedStyle>,
    style_changed: bool,
    changed_nodes: Vec<NodeKey>,
}

impl StyleEngine {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn sync_attrs_from_map(&mut self, map: &HashMap<NodeKey, HashMap<String, String>>) {
        self.attrs = map.clone();
    }
    pub fn rebuild_from_layout_snapshot(
        &mut self,
        _tags_by_key: &HashMap<NodeKey, String>,
        _element_children: &HashMap<NodeKey, Vec<NodeKey>>,
        _lay_attrs: &HashMap<NodeKey, HashMap<String, String>>,
    ) {
        // no-op for now
    }
    pub fn replace_stylesheet(&mut self, s: Stylesheet) {
        self.stylesheet = s;
    }
    pub fn force_full_restyle(&mut self) { /* no-op shim */
    }
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
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, ComputedStyle> {
        self.computed.clone()
    }
    pub fn take_and_clear_style_changed(&mut self) -> bool {
        let v = self.style_changed;
        self.style_changed = false;
        v
    }
    pub fn take_changed_nodes(&mut self) -> Vec<NodeKey> {
        take(&mut self.changed_nodes)
    }
}

impl DOMSubscriber for StyleEngine {
    fn apply_update(&mut self, _update: DOMUpdate) -> Result<()> {
        Ok(())
    }
}
