//! Component trait and function wrapper

use super::{Html, UiContext};

/// Trait for reactive components
pub trait Component: bevy::prelude::Component {
    /// Render function that produces HTML
    fn render(ctx: &mut UiContext<Self>) -> Html
    where
        Self: Sized;
}

/// Type alias for component functions
pub type ComponentFn<T> = fn(&mut UiContext<T>) -> Html;

/// Helper to check if a component implements the Component trait
pub trait IsComponent: bevy::prelude::Component {
    /// Check if this type implements Component
    fn is_component() -> bool {
        false
    }
}

impl<T: bevy::prelude::Component> IsComponent for T {}
