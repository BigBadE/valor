//! Extraction of layout-relevant values from computed styles.

use style_engine::{Display, SizeSpecified, Edges};
use js::NodeKey;

use super::args::LayoutMaps;

/// Information extracted from computed styles for a layout node.
#[derive(Debug, Clone)]
pub(crate) struct LayoutStyles {
    pub display_inline: bool,
    pub display_none: bool,
    pub display_flex: bool,
    pub display_inline_flex: bool,
    pub margin: Edges,
    pub padding: Edges,
    pub border: Edges,
    pub width_spec: Option<SizeSpecified>,
    pub height_spec: Option<SizeSpecified>,
}

impl Default for LayoutStyles {
    fn default() -> Self {
        Self {
            display_inline: false,
            display_none: false,
            display_flex: false,
            display_inline_flex: false,
            margin: Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
            padding: Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
            border: Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
            width_spec: None,
            height_spec: None,
        }
    }
}

/// Extract layout-relevant styles from computed styles for a given node.
pub(crate) fn extract_layout_styles(node: NodeKey, maps: &LayoutMaps) -> LayoutStyles {
    let mut styles = LayoutStyles::default();
    if let Some(computed_map) = maps.computed_by_key
        && let Some(computed_style) = computed_map.get(&node)
    {
        styles.display_none = computed_style.display == Display::None;
        styles.display_inline = computed_style.display == Display::Inline;
        styles.display_flex = computed_style.display == Display::Flex;
        styles.display_inline_flex = computed_style.display == Display::InlineFlex;
        styles.margin = computed_style.margin;
        styles.padding = computed_style.padding;
        styles.border = computed_style.border_width;
        styles.width_spec = Some(computed_style.width);
        styles.height_spec = Some(computed_style.height);
    }
    styles
}
