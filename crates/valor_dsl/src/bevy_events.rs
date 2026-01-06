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
    /// The entity that was clicked.
    pub entity: bevy::prelude::Entity,
}

/// Triggered when text input occurs.
#[derive(Event, Clone)]
pub struct OnInput {
    /// The input element node.
    pub node: NodeKey,
    /// The current value of the input.
    pub value: String,
}

/// Triggered when a form input value changes.
#[derive(Event, Clone)]
pub struct OnChange {
    /// The element node.
    pub node: NodeKey,
    /// The new value.
    pub value: String,
}

/// Triggered when a form is submitted.
#[derive(Event, Clone)]
pub struct OnSubmit {
    /// The form element node.
    pub node: NodeKey,
}

/// Triggered when an element gains focus.
#[derive(Event, Clone)]
pub struct OnFocus {
    /// The focused element node.
    pub node: NodeKey,
}

/// Triggered when an element loses focus.
#[derive(Event, Clone)]
pub struct OnBlur {
    /// The element node that lost focus.
    pub node: NodeKey,
}

/// Triggered when a key is pressed.
#[derive(Event, Clone)]
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
#[derive(Event, Clone)]
pub struct OnKeyUp {
    /// The focused element node.
    pub node: NodeKey,
    /// The key code.
    pub key: String,
}

/// Triggered when mouse enters an element.
#[derive(Event, Clone)]
pub struct OnMouseEnter {
    /// The element node.
    pub node: NodeKey,
    /// Mouse position.
    pub position: (f32, f32),
}

/// Triggered when mouse leaves an element.
#[derive(Event, Clone)]
pub struct OnMouseLeave {
    /// The element node.
    pub node: NodeKey,
}

/// Triggered when mouse moves over an element.
#[derive(Event, Clone)]
pub struct OnMouseMove {
    /// The element node.
    pub node: NodeKey,
    /// Mouse position.
    pub position: (f32, f32),
}

// Implement EntityEvent for OnClick to work with Bevy 0.17 observers
impl bevy::ecs::event::EntityEvent for OnClick {
    fn event_target(&self) -> bevy::prelude::Entity {
        self.entity
    }

    fn event_target_mut(&mut self) -> &mut bevy::prelude::Entity {
        &mut self.entity
    }
}
