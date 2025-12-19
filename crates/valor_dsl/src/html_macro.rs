//! HTML macro for creating Valor UIs with inline event handlers

/// Create HTML with event handler bindings
///
/// # Example
/// ```ignore
/// let ui = html! {
///     r#"
///     <div>
///         <h1>Counter</h1>
///         <button onclick="increment_counter">Increment</button>
///         <button onclick="decrement_counter">Decrement</button>
///     </div>
///     "#
/// };
/// ```
#[macro_export]
macro_rules! html {
    ($html:expr) => {
        $html.to_string()
    };
}

/// Register a Bevy observer function as a click handler
///
/// This creates an entity with a ClickHandler component that will receive OnClick events
/// when HTML elements with onclick="function_name" are clicked.
///
/// # Example
/// ```ignore
/// fn setup(mut commands: Commands) {
///     // Register the observer function
///     click_handler!(commands, increment_counter);
///
///     // HTML with onclick="increment_counter" will trigger this observer
/// }
///
/// fn increment_counter(_trigger: Trigger<OnClick>, mut q: Query<&mut Counter>) {
///     // Handle click event
/// }
/// ```
#[macro_export]
macro_rules! click_handler {
    ($commands:expr, $func:ident) => {
        $commands.spawn($crate::bevy_integration::ClickHandler {
            name: stringify!($func).to_string(),
        });
    };
}
