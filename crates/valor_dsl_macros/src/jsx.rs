//! JSX parsing and code generation

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::{braced, Expr, Ident, LitStr, Result, Token};

/// Root JSX structure - can contain multiple children
pub struct Jsx {
    pub children: Vec<JsxChild>,
}

impl Parse for Jsx {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut children = Vec::new();

        while !input.is_empty() {
            children.push(input.parse()?);
        }

        Ok(Jsx { children })
    }
}

impl Jsx {
    /// Convert JSX to code that builds an HTML string
    pub fn to_html_string(&self) -> TokenStream {
        let children_code = self.children.iter().map(|child| child.to_html_string());

        quote! {
            #(#children_code)*
        }
    }
}

/// A child node in JSX - can be an element, text, or Rust expression
pub enum JsxChild {
    /// HTML element like <div>...</div>
    Element(JsxElement),
    /// Plain text content
    Text(LitStr),
    /// Rust expression in braces {expr}
    Expression(Expr),
}

impl Parse for JsxChild {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![<]) {
            Ok(JsxChild::Element(input.parse()?))
        } else if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            Ok(JsxChild::Expression(content.parse()?))
        } else if input.peek(LitStr) {
            Ok(JsxChild::Text(input.parse()?))
        } else {
            Err(input.error("expected element, text, or expression"))
        }
    }
}

impl JsxChild {
    fn to_html_string(&self) -> TokenStream {
        match self {
            JsxChild::Element(el) => el.to_html_string(),
            JsxChild::Text(text) => {
                let text_val = text.value();
                quote! {
                    __html.push_str(#text_val);
                }
            }
            JsxChild::Expression(expr) => {
                // Expression blocks can return any Display type (including Html from nested jsx!)
                quote! {
                    {
                        use std::fmt::Write;
                        let _ = write!(__html, "{}", #expr);
                    }
                }
            }
        }
    }
}

/// An HTML element with tag, attributes, and children
pub struct JsxElement {
    pub tag: Ident,
    pub attributes: Vec<JsxAttribute>,
    pub children: Vec<JsxChild>,
    pub self_closing: bool,
}

impl Parse for JsxElement {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse opening tag: <tag
        input.parse::<Token![<]>()?;
        let tag: Ident = input.parse()?;

        // Parse attributes
        let mut attributes = Vec::new();
        while !input.peek(Token![>]) && !input.peek(Token![/]) {
            attributes.push(input.parse()?);
        }

        // Check for self-closing tag: />
        let self_closing = if input.peek(Token![/]) {
            input.parse::<Token![/]>()?;
            input.parse::<Token![>]>()?;
            true
        } else {
            input.parse::<Token![>]>()?;
            false
        };

        // Parse children if not self-closing
        let children = if self_closing {
            Vec::new()
        } else {
            let mut children = Vec::new();

            // Parse children until we hit the closing tag
            while !input.peek(Token![<]) || !input.peek2(Token![/]) {
                // Stop if we're at EOF
                if input.is_empty() {
                    return Err(input.error("unexpected end of input, expected closing tag"));
                }

                // Check if this is the closing tag
                let fork = input.fork();
                if fork.peek(Token![<]) && fork.peek2(Token![/]) {
                    break;
                }

                children.push(input.parse()?);
            }

            // Parse closing tag: </tag>
            input.parse::<Token![<]>()?;
            input.parse::<Token![/]>()?;
            let close_tag: Ident = input.parse()?;
            input.parse::<Token![>]>()?;

            // Verify tags match
            if tag != close_tag {
                return Err(input.error(format!("mismatched tags: <{tag}> and </{close_tag}>")));
            }

            children
        };

        Ok(JsxElement {
            tag,
            attributes,
            children,
            self_closing,
        })
    }
}

impl JsxElement {
    fn to_html_string(&self) -> TokenStream {
        let tag = self.tag.to_string();
        let attrs_code = self.attributes.iter().map(|attr| attr.to_html_string());
        let children_code = self.children.iter().map(|child| child.to_html_string());

        if self.self_closing {
            quote! {
                __html.push_str(concat!("<", #tag));
                #(#attrs_code)*
                __html.push_str(" />");
            }
        } else {
            quote! {
                __html.push_str(concat!("<", #tag));
                #(#attrs_code)*
                __html.push_str(">");
                #(#children_code)*
                __html.push_str(concat!("</", #tag, ">"));
            }
        }
    }
}

/// An attribute on an HTML element
pub enum JsxAttribute {
    /// Static attribute: class="foo"
    Static { name: Ident, value: LitStr },
    /// Dynamic attribute: onclick={handler}
    Dynamic { name: Ident, value: Expr },
}

impl Parse for JsxAttribute {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;

        if input.peek(LitStr) {
            let value: LitStr = input.parse()?;
            Ok(JsxAttribute::Static { name, value })
        } else if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            let value: Expr = content.parse()?;
            Ok(JsxAttribute::Dynamic { name, value })
        } else {
            Err(input.error("expected string literal or expression in braces"))
        }
    }
}

impl JsxAttribute {
    fn to_html_string(&self) -> TokenStream {
        match self {
            JsxAttribute::Static { name, value } => {
                let name_str = name.to_string();
                let value_str = value.value();
                quote! {
                    __html.push_str(concat!(" ", #name_str, "=\"", #value_str, "\""));
                }
            }
            JsxAttribute::Dynamic { name, value } => {
                let name_str = name.to_string();
                quote! {
                    {
                        use std::fmt::Write;
                        write!(__html, " {}=\"{}\"", #name_str, #value).unwrap();
                    }
                }
            }
        }
    }
}
