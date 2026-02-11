//! Reactive component system for Valor DSL
//!
//! This module provides a React-like component API for building UIs with automatic
//! reactivity, state management, and event handling integrated with Bevy ECS.

pub mod component;
pub mod context;
pub mod html;
pub mod runtime;

pub use component::{Component, ComponentFn};
pub use context::UiContext;
pub use html::Html;
pub use runtime::ReactiveUiPlugin;

/// Prelude for reactive components
pub mod prelude {
    pub use super::{Component, ComponentFn, Html, UiContext};
}
