//! Procedural macros for Valor DSL
//!
//! This crate provides JSX-like syntax for building HTML UIs with Rust.

use proc_macro::TokenStream;
use quote::quote;

mod jsx;

/// JSX-like macro for building HTML with Rust expressions
///
/// Generates code that produces `Html` with `DOMUpdate` operations.
///
/// # Examples
///
/// ```ignore
/// jsx! {
///     <div class="container">
///         <h1>Hello, {name}!</h1>
///         {if count > 0 {
///             <p>Count: {count}</p>
///         }}
///     </div>
/// }
/// ```
#[proc_macro]
pub fn jsx(input: TokenStream) -> TokenStream {
    let jsx = syn::parse_macro_input!(input as jsx::Jsx);

    let output = jsx.to_dom_updates();

    TokenStream::from(quote! {
        {
            use ::js::{DOMUpdate, NodeKey};
            use ::std::collections::HashMap;

            // Use stable IDs (no epoch) so nodes have consistent IDs across renders
            // This allows us to identify and update the same nodes
            let __epoch = 0u16;

            let mut __updates = Vec::new();
            let mut __event_handlers: HashMap<NodeKey, HashMap<String, String>> = HashMap::new();
            let mut __next_id: usize = 1; // Start from 1 to avoid colliding with ROOT (0)

            // Generate the DOM updates
            #output

            ::valor_dsl::reactive::Html::with_handlers(__updates, __event_handlers)
        }
    })
}
