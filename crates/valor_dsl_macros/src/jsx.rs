//! JSX parsing and code generation

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Result, Token, braced};

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
    /// Convert JSX to code that generates DOMUpdate operations
    pub fn to_dom_updates(&self) -> TokenStream {
        let children_code = self.children.iter().map(|child| {
            child.to_dom_updates(quote! { NodeKey::ROOT }, quote! { __updates.len() })
        });

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
    fn to_dom_updates(&self, parent: TokenStream, pos: TokenStream) -> TokenStream {
        match self {
            JsxChild::Element(el) => el.to_dom_updates(parent, pos),
            JsxChild::Text(text) => {
                let text_val = text.value();
                quote! {
                    {
                        let node_key = NodeKey::pack(__epoch, 0, __next_id as u64);
                        __next_id += 1;
                        __updates.push(DOMUpdate::InsertText {
                            parent: #parent,
                            node: node_key,
                            text: #text_val.to_string(),
                            pos: #pos,
                        });
                    }
                }
            }
            JsxChild::Expression(expr) => {
                // Expression blocks can return Html (from nested jsx!) or any Display type
                quote! {
                    {
                        let __expr_result = #expr;
                        // If it's Html, merge it with proper node ID remapping
                        if let Some(__html_result) = (|| -> Option<::valor_dsl::reactive::Html> {
                            // Try to convert to Html
                            use ::std::any::Any;
                            if let Some(h) = (&__expr_result as &dyn Any).downcast_ref::<::valor_dsl::reactive::Html>() {
                                Some(h.clone())
                            } else {
                                None
                            }
                        })() {
                            // Remap all node IDs in the nested Html to avoid conflicts
                            let __id_offset = __next_id as u64;

                            for mut update in __html_result.updates {
                                // Helper to remap a node key if it's not ROOT
                                let remap = |key: NodeKey| -> NodeKey {
                                    if key == NodeKey::ROOT {
                                        #parent  // Keep ROOT as the actual parent
                                    } else {
                                        // Add offset to avoid ID collisions
                                        NodeKey::pack(0, 0, key.0 + __id_offset)
                                    }
                                };

                                // Remap all node references in the update
                                match &mut update {
                                    DOMUpdate::InsertElement { parent: p, node: n, pos: pos_ref, .. } => {
                                        *p = remap(*p);
                                        *n = remap(*n);
                                        if *p == #parent {
                                            *pos_ref = #pos;
                                        }
                                        // Track the highest ID we've seen
                                        let unpacked = n.0;
                                        if unpacked >= __next_id as u64 {
                                            __next_id = (unpacked + 1) as usize;
                                        }
                                    }
                                    DOMUpdate::InsertText { parent: p, node: n, pos: pos_ref, .. } => {
                                        *p = remap(*p);
                                        *n = remap(*n);
                                        if *p == #parent {
                                            *pos_ref = #pos;
                                        }
                                        let unpacked = n.0;
                                        if unpacked >= __next_id as u64 {
                                            __next_id = (unpacked + 1) as usize;
                                        }
                                    }
                                    DOMUpdate::SetAttr { node: n, .. } => {
                                        *n = remap(*n);
                                    }
                                    DOMUpdate::RemoveNode { node: n } => {
                                        *n = remap(*n);
                                    }
                                    DOMUpdate::UpdateText { node: n, .. } => {
                                        *n = remap(*n);
                                    }
                                    _ => {}
                                }
                                __updates.push(update);
                            }

                            // Remap event handler node keys too
                            for (node, handlers) in __html_result.event_handlers {
                                let remapped_node = if node == NodeKey::ROOT {
                                    #parent
                                } else {
                                    NodeKey::pack(0, 0, node.0 + __id_offset)
                                };
                                __event_handlers.entry(remapped_node).or_default().extend(handlers);
                            }
                        } else {
                            // Otherwise treat as text
                            let node_key = NodeKey::pack(__epoch, 0, __next_id as u64);
                            __next_id += 1;
                            __updates.push(DOMUpdate::InsertText {
                                parent: #parent,
                                node: node_key,
                                text: format!("{}", __expr_result),
                                pos: #pos,
                            });
                        }
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
        })
    }
}

impl JsxElement {
    fn to_dom_updates(&self, parent: TokenStream, pos: TokenStream) -> TokenStream {
        let tag = self.tag.to_string();

        // Generate code for attributes
        let attrs_code = self.attributes.iter().map(|attr| attr.to_dom_updates());

        // Generate code for children with incremental positions
        let children_code = self
            .children
            .iter()
            .enumerate()
            .map(|(i, child)| child.to_dom_updates(quote! { __parent_key }, quote! { #i }));

        quote! {
            {
                // Create node for this element with unique epoch
                let __elem_key = NodeKey::pack(__epoch, 0, __next_id as u64);
                __next_id += 1;

                __updates.push(DOMUpdate::InsertElement {
                    parent: #parent,
                    node: __elem_key,
                    tag: #tag.to_string(),
                    pos: #pos,
                });

                // Apply attributes
                #(#attrs_code)*

                // Add children (use __parent_key to refer to this element)
                let __parent_key = __elem_key;
                #(#children_code)*
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
    fn to_dom_updates(&self) -> TokenStream {
        match self {
            JsxAttribute::Static { name, value } => {
                let name_str = name.to_string();
                let value_str = value.value();

                // Check if this is an onclick handler
                if name_str.starts_with("onclick") {
                    quote! {
                        __event_handlers
                            .entry(__elem_key)
                            .or_default()
                            .insert("click".to_string(), #value_str.to_string());

                        __updates.push(DOMUpdate::SetAttr {
                            node: __elem_key,
                            name: #name_str.to_string(),
                            value: #value_str.to_string(),
                        });
                    }
                } else {
                    quote! {
                        __updates.push(DOMUpdate::SetAttr {
                            node: __elem_key,
                            name: #name_str.to_string(),
                            value: #value_str.to_string(),
                        });
                    }
                }
            }
            JsxAttribute::Dynamic { name, value } => {
                let name_str = name.to_string();

                // Check if this is an onclick handler
                if name_str.starts_with("onclick") {
                    quote! {
                        {
                            let __attr_value = format!("{}", #value);
                            __event_handlers
                                .entry(__elem_key)
                                .or_default()
                                .insert("click".to_string(), __attr_value.clone());

                            __updates.push(DOMUpdate::SetAttr {
                                node: __elem_key,
                                name: #name_str.to_string(),
                                value: __attr_value,
                            });
                        }
                    }
                } else {
                    quote! {
                        __updates.push(DOMUpdate::SetAttr {
                            node: __elem_key,
                            name: #name_str.to_string(),
                            value: format!("{}", #value),
                        });
                    }
                }
            }
        }
    }
}
