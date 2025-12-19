//! Reactive HTML macro for interpolating Rust expressions into HTML

/// Macro for creating reactive HTML with Rust expression interpolation
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
    // Base case: just a string literal
    ($html:literal) => {
        $crate::reactive::Html::new($html)
    };

    // Handle HTML with interpolations
    ($($tokens:tt)*) => {{
        let mut html_string = String::new();
        $crate::__reactive_html_impl!(html_string; $($tokens)*);
        $crate::reactive::Html::new(html_string)
    }};
}

/// Internal implementation macro for reactive_html
#[doc(hidden)]
#[macro_export]
macro_rules! __reactive_html_impl {
    // Base case: empty
    ($output:ident;) => {};

    // Rust expression interpolation: {expr} - must come before literal to match first
    ($output:ident; {$expr:expr} $($rest:tt)*) => {
        {
            use std::fmt::Write;
            let _ = write!($output, "{}", $expr);
        }
        $crate::__reactive_html_impl!($output; $($rest)*);
    };

    // String literal (works for both r#"..."# and "...")
    ($output:ident; $lit:tt $($rest:tt)*) => {
        $output.push_str($lit);
        $crate::__reactive_html_impl!($output; $($rest)*);
    };
}

/// Simplified reactive_html! for full HTML documents (wraps in html! style)
///
/// This is a convenience macro that works like the existing html! macro
/// but supports the reactive_html! interpolation syntax
#[macro_export]
macro_rules! rhtml {
    ($($tokens:tt)*) => {
        $crate::reactive_html!($($tokens)*)
    };
}

#[cfg(test)]
mod tests {
    use crate::reactive::Html;

    #[test]
    fn test_reactive_html_literal() {
        let html = reactive_html!("<div>Hello</div>");
        assert_eq!(html.content, "<div>Hello</div>");
    }

    #[test]
    fn test_reactive_html_interpolation() {
        let name = "World";
        let html = reactive_html! {
            "<h1>Hello, " {name} "!</h1>"
        };
        assert_eq!(html.content, "<h1>Hello, World!</h1>");
    }

    #[test]
    fn test_reactive_html_multiple_interpolations() {
        let x = 10;
        let y = 20;
        let html = reactive_html! {
            "<div>X: " {x} ", Y: " {y} "</div>"
        };
        assert_eq!(html.content, "<div>X: 10, Y: 20</div>");
    }
}
