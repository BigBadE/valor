//! Bevy event triggers for Valor UI interactions.

use bevy::prelude::*;
use js::NodeKey;

/// Triggered when an element is clicked.
#[derive(Event, Clone)]
pub struct OnClick {
    /// The node that was clicked.
    pub node: NodeKey,
    /// Mouse position (x, y) in viewport coordinates.
    pub position: (f32, f32),
    /// Which mouse button was clicked (0=left, 1=middle, 2=right).
    pub button: u8,
}

/// Triggered when text input occurs.
#[derive(Event)]
pub struct OnInput {
    /// The input element node.
    pub node: NodeKey,
    /// The current value of the input.
    pub value: String,
}

/// Triggered when a form input value changes.
#[derive(Event)]
pub struct OnChange {
    /// The element node.
    pub node: NodeKey,
    /// The new value.
    pub value: String,
}

/// Triggered when a form is submitted.
#[derive(Event)]
pub struct OnSubmit {
    /// The form element node.
    pub node: NodeKey,
}

/// Triggered when an element gains focus.
#[derive(Event)]
pub struct OnFocus {
    /// The focused element node.
    pub node: NodeKey,
}

/// Triggered when an element loses focus.
#[derive(Event)]
pub struct OnBlur {
    /// The element node that lost focus.
    pub node: NodeKey,
}

/// Triggered when a key is pressed.
#[derive(Event)]
pub struct OnKeyDown {
    /// The focused element node.
    pub node: NodeKey,
    /// The key code.
    pub key: String,
    /// Whether Ctrl is pressed.
    pub ctrl: bool,
    /// Whether Shift is pressed.
    pub shift: bool,
    /// Whether Alt is pressed.
    pub alt: bool,
}

/// Triggered when a key is released.
#[derive(Event)]
pub struct OnKeyUp {
    /// The focused element node.
    pub node: NodeKey,
    /// The key code.
    pub key: String,
}

/// Triggered when mouse enters an element.
#[derive(Event)]
pub struct OnMouseEnter {
    /// The element node.
    pub node: NodeKey,
    /// Mouse position.
    pub position: (f32, f32),
}

/// Triggered when mouse leaves an element.
#[derive(Event)]
pub struct OnMouseLeave {
    /// The element node.
    pub node: NodeKey,
}

/// Triggered when mouse moves over an element.
#[derive(Event)]
pub struct OnMouseMove {
    /// The element node.
    pub node: NodeKey,
    /// Mouse position.
    pub position: (f32, f32),
}
