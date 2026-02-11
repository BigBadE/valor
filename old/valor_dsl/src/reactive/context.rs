//! UI Context for reactive components

use bevy::prelude::*;
use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

/// Callback function type for event handlers (component-only mutations)
pub type CallbackFn<T> = Arc<dyn Fn(&mut T) + Send + Sync>;

/// UI Context providing state access and event handler registration
pub struct UiContext<'w, T: Component> {
    /// Reference to the component state
    pub state: &'w T,
    /// Entity owning this component
    pub entity: Entity,
    /// World reference for queries
    world: &'w World,
    /// Registered callbacks for this component (accumulated during render)
    pub(crate) callbacks: HashMap<String, CallbackFn<T>>,
    /// Phantom data for component type
    _phantom: PhantomData<T>,
}

impl<'w, T: Component> UiContext<'w, T> {
    /// Create a new UI context
    #[inline]
    pub fn new(state: &'w T, entity: Entity, world: &'w World) -> Self {
        Self {
            state,
            entity,
            world,
            callbacks: HashMap::new(),
            _phantom: PhantomData,
        }
    }

    /// Access the component state
    #[inline]
    pub const fn use_state(&self) -> &T {
        self.state
    }

    /// Register a callback that mutates the component state
    ///
    /// The callback receives mutable access to the component.
    pub fn callback(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> String {
        let name = name.into();
        self.callbacks.insert(name.clone(), Arc::new(f));
        name
    }

    /// Register a click handler callback that mutates the component
    /// This is an alias for `callback` for better DX
    pub fn on_click(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&mut T) + Send + Sync + 'static,
    ) -> String {
        self.callback(name, f)
    }

    /// Get a reference to the world for queries
    #[inline]
    pub const fn world(&self) -> &World {
        self.world
    }

    /// Get all registered callbacks
    #[inline]
    pub fn callbacks(&self) -> &HashMap<String, CallbackFn<T>> {
        &self.callbacks
    }

    /// Take ownership of callbacks (used during component spawn)
    #[inline]
    pub fn take_callbacks(self) -> HashMap<String, CallbackFn<T>> {
        self.callbacks
    }
}

/// Marker component indicating this entity has reactive callbacks
#[derive(Component)]
pub struct ReactiveCallbacks<T: Component> {
    /// Type-erased callbacks
    pub handlers: HashMap<String, CallbackFn<T>>,
    /// TypeId for runtime type checking
    pub type_id: TypeId,
}

impl<T: Component> ReactiveCallbacks<T> {
    /// Create new reactive callbacks container
    #[inline]
    pub fn new(handlers: HashMap<String, CallbackFn<T>>) -> Self {
        Self {
            handlers,
            type_id: TypeId::of::<T>(),
        }
    }

    /// Get a callback by name
    #[inline]
    pub fn get(&self, name: &str) -> Option<&CallbackFn<T>> {
        self.handlers.get(name)
    }
}
