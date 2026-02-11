//! Reactive HTML macro for interpolating Rust expressions into HTML

/// Macro for creating reactive HTML with Rust expression interpolation
///
/// **NOTE**: This macro is currently unimplemented. The Html API changed from
/// `Html::new(String)` to `Html::new(Vec<DOMUpdate>)`, which requires parsing
/// the HTML string into DOMUpdate events.
///
/// To properly implement this macro, we need to:
/// 1. Parse the HTML string using html5ever
/// 2. Generate DOMUpdate events from the parsed tree
/// 3. Handle string interpolation by generating text nodes
///
/// For now, this macro will panic if called.
///
/// # Examples
///
/// ```ignore
/// let name = "World";
/// let count = 42;
///
/// let html = reactive_html! {
///     <div>
///         <h1>"Hello, " {name}</h1>
///         <p>"Count: " {count}</p>
///     </div>
/// };
/// ```
#[macro_export]
macro_rules! reactive_html {
    ($($tokens:tt)*) => {{
        compile_error!(
            "reactive_html! macro is not yet implemented for the new Html API. \
             The Html struct now requires Vec<DOMUpdate> instead of String. \
             Please use the html! macro from valor_dsl_macros or construct \
             Html::new() with DOMUpdate events directly."
        )
    }};
}

/// Simplified reactive_html! for full HTML documents (wraps in html! style)
///
/// **NOTE**: Also unimplemented, same as reactive_html!
#[macro_export]
macro_rules! rhtml {
    ($($tokens:tt)*) => {
        compile_error!(
            "rhtml! macro is not yet implemented. Use html! from valor_dsl_macros instead."
        )
    };
}

#[cfg(test)]
mod tests {
    // Tests disabled - reactive_html! macro needs to be rewritten
    // to work with the new Html::new(Vec<DOMUpdate>) API instead of
    // Html::new(String)
    //
    // TODO: Rewrite macro to generate DOMUpdate objects directly from HTML string
}
