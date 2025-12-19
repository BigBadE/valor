//! Macros for easier Valor DSL usage

/// Register an event callback using a closure or function
///
/// This macro makes it easy to register event handlers inline without
/// manually creating closures and calling `EventCallbacks::register`.
///
/// # Examples
///
/// ```ignore
/// use valor_dsl::valor_callback;
///
/// let mut callbacks = EventCallbacks::new();
///
/// // Using a closure
/// valor_callback!(callbacks, "increment", |ctx| {
///     println!("Increment clicked!");
/// });
///
/// // Using a function
/// fn handle_click(ctx: &EventContext) {
///     println!("Button clicked!");
/// }
/// valor_callback!(callbacks, "click", handle_click);
///
/// // With captured variables
/// let counter = Arc::new(Mutex::new(0));
/// valor_callback!(callbacks, "increment", [counter], |ctx| {
///     if let Ok(mut count) = counter.lock() {
///         *count += 1;
///     }
/// });
/// ```
#[macro_export]
macro_rules! valor_callback {
    // Closure without captures
    ($callbacks:expr, $name:expr, |$ctx:ident| $body:expr) => {
        $callbacks.register($name, move |$ctx: &$crate::events::EventContext| {
            $body
        });
    };

    // Closure with captures
    ($callbacks:expr, $name:expr, [$($capture:ident),*], |$ctx:ident| $body:expr) => {
        $(let $capture = ::std::clone::Clone::clone(&$capture);)*
        $callbacks.register($name, move |$ctx: &$crate::events::EventContext| {
            $body
        });
    };

    // Function reference
    ($callbacks:expr, $name:expr, $func:expr) => {
        $callbacks.register($name, $func);
    };
}

/// Helper macro to create HTML with inline event callbacks
///
/// This macro allows you to define HTML with Rust closures inline,
/// making it easier to write interactive UIs.
///
/// # Examples
///
/// ```ignore
/// use valor_dsl::valor_html;
///
/// let (html, callbacks) = valor_html! {
///     r#"
///     <button on:click="increment">Click me!</button>
///     "#,
///     callbacks: {
///         "increment" => |ctx| {
///             println!("Clicked!");
///         }
///     }
/// };
/// ```
#[macro_export]
macro_rules! valor_html {
    (
        $html:expr,
        callbacks: {
            $($name:expr => $handler:expr),* $(,)?
        }
    ) => {{
        let mut callbacks = $crate::events::EventCallbacks::new();
        $(
            callbacks.register($name, $handler);
        )*
        ($html.to_string(), callbacks)
    }};
}

/// Create a Valor UI plugin with HTML and callbacks
///
/// # Examples
///
/// ```ignore
/// use valor_dsl::valor_plugin;
///
/// let plugin = valor_plugin! {
///     html: r#"<button on:click="test">Click</button>"#,
///     callbacks: {
///         "test" => |_| println!("Clicked!")
///     },
///     width: 800,
///     height: 600
/// };
/// ```
#[macro_export]
macro_rules! valor_plugin {
    (
        html: $html:expr,
        callbacks: {
            $($name:expr => $handler:expr),* $(,)?
        }
        $(, width: $width:expr)?
        $(, height: $height:expr)?
        $(, capture_mouse: $mouse:expr)?
        $(, capture_keyboard: $keyboard:expr)?
    ) => {{
        let mut callbacks = $crate::events::EventCallbacks::new();
        $(
            callbacks.register($name, $handler);
        )*

        $crate::bevy_integration::ValorUiPlugin {
            initial_html: $html.to_string(),
            callbacks,
            width: valor_plugin!(@default_width $($width)?),
            height: valor_plugin!(@default_height $($height)?),
            ..Default::default()
        }
    }};

    (@default_width) => { 1024 };
    (@default_width $width:expr) => { $width };

    (@default_height) => { 768 };
    (@default_height $height:expr) => { $height };
}
