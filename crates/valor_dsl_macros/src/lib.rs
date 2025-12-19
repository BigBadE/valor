//! Procedural macros for Valor DSL
//!
//! This crate provides JSX-like syntax for building HTML UIs with Rust.

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Token, Result};

mod jsx;

/// JSX-like macro for building HTML with Rust expressions
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

    let output = jsx.to_html_string();

    TokenStream::from(quote! {
        {
            let mut __html = String::new();
            #output
            ::valor_dsl::reactive::Html::new(__html)
        }
    })
}
