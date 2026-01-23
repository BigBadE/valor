//! CSS property input implementation.

use super::store::get_initial_value;
use crate::CssValue;
use rewrite_core::{Input, NodeId};

/// Input for CSS property values.
pub struct CssPropertyInput;

impl Input for CssPropertyInput {
    type Key = (NodeId, String);
    type Value = CssValue;

    fn name() -> &'static str {
        "CssPropertyInput"
    }

    fn default_value(key: &Self::Key) -> Self::Value {
        let (_node, property) = key;
        get_initial_value(property)
    }
}

/// Input for the root element node ID (the <html> element).
/// Key is () (unit) since there's only one root.
/// Value is the NodeId of the root element.
pub struct RootElementInput;

impl Input for RootElementInput {
    type Key = ();
    type Value = NodeId;

    fn name() -> &'static str {
        "RootElementInput"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        // Default to node 0 if not set (shouldn't happen in practice)
        NodeId::new(0)
    }
}

/// Viewport dimensions in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSize {
    pub width: f32,
    pub height: f32,
}

impl Default for ViewportSize {
    fn default() -> Self {
        Self {
            width: 1920.0,
            height: 1080.0,
        }
    }
}

/// Input for viewport dimensions.
/// Key is () (unit) since there's only one viewport.
pub struct ViewportInput;

impl Input for ViewportInput {
    type Key = ();
    type Value = ViewportSize;

    fn name() -> &'static str {
        "ViewportInput"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        // Default viewport size (common desktop)
        ViewportSize {
            width: 1920.0,
            height: 1080.0,
        }
    }
}
