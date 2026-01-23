//! CSS property storage.

use crate::{ColorValue, CssKeyword, CssValue, LengthValue};
use dashmap::DashMap;
use rewrite_core::NodeId;
use std::sync::Arc;

/// Storage for CSS property values.
pub struct CssStorage {
    properties: DashMap<(NodeId, String), Arc<CssValue>>,
}

impl CssStorage {
    pub fn new() -> Self {
        Self {
            properties: DashMap::new(),
        }
    }

    pub fn set_property(&self, node: NodeId, property: String, value: CssValue) {
        self.properties.insert((node, property), Arc::new(value));
    }

    pub fn get_property(&self, node: NodeId, property: &str) -> Option<Arc<CssValue>> {
        self.properties
            .get(&(node, property.to_string()))
            .map(|entry| entry.value().clone())
    }

    pub fn clear_node(&self, node: NodeId) {
        self.properties.retain(|(n, _), _| *n != node);
    }

    pub fn clear_all(&self) {
        self.properties.clear();
    }
}

impl Default for CssStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Get initial (default) value for a CSS property.
pub fn get_initial_value(property: &str) -> CssValue {
    use super::properties::*;

    match property {
        PADDING_TOP | PADDING_RIGHT | PADDING_BOTTOM | PADDING_LEFT => {
            CssValue::Length(LengthValue::Px(0.0))
        }
        MARGIN_TOP | MARGIN_RIGHT | MARGIN_BOTTOM | MARGIN_LEFT => {
            CssValue::Length(LengthValue::Px(0.0))
        }
        BORDER_TOP_WIDTH | BORDER_RIGHT_WIDTH | BORDER_BOTTOM_WIDTH | BORDER_LEFT_WIDTH => {
            CssValue::Length(LengthValue::Px(0.0))
        }
        TOP | RIGHT | BOTTOM | LEFT => CssValue::Keyword(CssKeyword::Auto),
        WIDTH | HEIGHT | MIN_WIDTH | MIN_HEIGHT | MAX_WIDTH | MAX_HEIGHT => {
            CssValue::Keyword(CssKeyword::Auto)
        }
        ROW_GAP | COLUMN_GAP | GAP => CssValue::Length(LengthValue::Px(0.0)),
        FLEX_GROW => CssValue::Number(0.0),
        FLEX_SHRINK => CssValue::Number(1.0),
        FONT_SIZE => CssValue::Length(LengthValue::Px(16.0)),
        COLOR => CssValue::Color(ColorValue {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }),
        "display" => CssValue::Keyword(CssKeyword::Block),
        "position" => CssValue::Keyword(CssKeyword::Static),
        "float" => CssValue::Keyword(CssKeyword::None),
        "clear" => CssValue::Keyword(CssKeyword::None),
        "overflow" => CssValue::Keyword(CssKeyword::Visible),
        "overflow-x" => CssValue::Keyword(CssKeyword::Visible),
        "overflow-y" => CssValue::Keyword(CssKeyword::Visible),
        "visibility" => CssValue::Keyword(CssKeyword::Visible),
        _ => CssValue::Keyword(CssKeyword::Auto),
    }
}
