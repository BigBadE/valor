use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Meta, Type, parse_macro_input};

#[proc_macro_derive(Markers)]
pub fn derive_markers(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = &input.ident;

    let Data::Enum(data_enum) = &input.data else {
        panic!("Markers can only be derived for enums");
    };

    let trait_name = syn::Ident::new(&format!("{}Marker", enum_name), enum_name.span());

    let mut marker_structs = Vec::new();
    let mut marker_impls = Vec::new();

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;

        // Only support unit variants
        if !matches!(variant.fields, Fields::Unit) {
            panic!("Markers derive only supports unit variants");
        }

        let marker_struct_name =
            syn::Ident::new(&format!("{}Marker", variant_name), variant_name.span());

        marker_structs.push(quote! {
            pub struct #marker_struct_name;
        });

        marker_impls.push(quote! {
            impl #trait_name for #marker_struct_name {
                fn to_value() -> #enum_name {
                    #enum_name::#variant_name
                }
            }
        });
    }

    let expanded = quote! {
        pub trait #trait_name {
            fn to_value() -> #enum_name;
        }

        #(#marker_structs)*

        #(#marker_impls)*
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(EnumQuery, attributes(query, value_type, params))]
pub fn derive_enum_query(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = &input.ident;

    let Data::Enum(data_enum) = &input.data else {
        panic!("EnumQuery can only be derived for enums");
    };

    // Find the #[value_type(TypeName)] attribute on the enum itself
    let value_type = input
        .attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("value_type") {
                let Meta::List(meta_list) = &attr.meta else {
                    panic!("value_type attribute must be in the form #[value_type(TypeName)]");
                };
                let type_name: Type = meta_list.parse_args().expect("Expected type name");
                Some(type_name)
            } else {
                None
            }
        })
        .expect("EnumQuery derive requires a #[value_type(TypeName)] attribute on the enum");

    // Collect all variants and their query functions
    let mut variant_info = Vec::new();
    let mut common_params = None;

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;

        // Must be unit variant
        if !matches!(variant.fields, Fields::Unit) {
            panic!("EnumQuery variants must be unit variants");
        }

        // Find the #[query(function_name)] attribute
        let query_fn = variant
            .attrs
            .iter()
            .find_map(|attr| {
                if attr.path().is_ident("query") {
                    let Meta::List(meta_list) = &attr.meta else {
                        panic!("query attribute must be in the form #[query(function_name)]");
                    };
                    let fn_name: syn::Path =
                        meta_list.parse_args().expect("Expected function name");
                    Some(fn_name)
                } else {
                    None
                }
            })
            .expect(&format!(
                "Variant {} must have a #[query(function_name)] attribute",
                variant_name
            ));

        // Check for #[params(...)] attribute
        let param_traits = variant.attrs.iter().find_map(|attr| {
            if attr.path().is_ident("params") {
                let Meta::List(meta_list) = &attr.meta else {
                    return None;
                };
                let traits = meta_list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<Type, syn::Token![,]>::parse_terminated,
                    )
                    .ok()?;
                Some(traits.into_iter().collect::<Vec<_>>())
            } else {
                None
            }
        });

        // Verify all variants have the same number of params
        match (&common_params, &param_traits) {
            (None, Some(params)) => common_params = Some(params.clone()),
            (Some(common), Some(current)) if common.len() == current.len() => {}
            (Some(_), Some(_)) => {
                panic!("All variants must have the same number of params for EnumQuery")
            }
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                panic!(
                    "All variants must consistently have or not have #[params(...)] for EnumQuery"
                )
            }
        }

        variant_info.push((variant_name, query_fn));
    }

    // Generate the unified query struct
    let query_struct_name = syn::Ident::new(&format!("{}Query", enum_name), enum_name.span());

    let enum_marker_trait = syn::Ident::new(&format!("{}Marker", enum_name), enum_name.span());

    let expanded = match common_params {
        None => {
            // No parameters - generate simple query that takes just the enum
            let match_arms = variant_info.iter().map(|(variant_name, query_fn)| {
                quote! {
                    #enum_name::#variant_name => #query_fn(db, node, ctx),
                }
            });

            quote! {
                pub struct #query_struct_name<Variant>(::std::marker::PhantomData<Variant>);

                impl<Variant> rewrite_core::Query for #query_struct_name<Variant>
                where
                    Variant: #enum_marker_trait + 'static,
                {
                    type Key = rewrite_core::NodeId;
                    type Value = #value_type;

                    fn execute(db: &rewrite_core::Database, node: rewrite_core::NodeId, ctx: &mut rewrite_core::DependencyContext) -> Self::Value {
                        match Variant::to_value() {
                            #(#match_arms)*
                        }
                    }
                }
            }
        }
        Some(ref params) => {
            // Generate parameter names and trait bounds dynamically
            let param_count = params.len();
            let param_names: Vec<_> = (1..=param_count)
                .map(|i| syn::Ident::new(&format!("Param{}", i), enum_name.span()))
                .collect();

            // Build the to_value() calls for each parameter
            let param_values: Vec<_> = param_names
                .iter()
                .map(|p| quote! { #p::to_value() })
                .collect();

            // Build match arms that call the query function with all parameters
            let match_arms = variant_info.iter().map(|(variant_name, query_fn)| {
                quote! {
                    #enum_name::#variant_name => #query_fn(db, node, #(#param_values),*, ctx),
                }
            });

            // Build where clause bounds
            let where_bounds = param_names
                .iter()
                .zip(params.iter())
                .map(|(name, trait_bound)| {
                    quote! { #name: #trait_bound + 'static }
                });

            quote! {
                pub struct #query_struct_name<Variant, #(#param_names),*>(::std::marker::PhantomData<(Variant, #(#param_names),*)>);

                impl<Variant, #(#param_names),*> rewrite_core::Query for #query_struct_name<Variant, #(#param_names),*>
                where
                    Variant: #enum_marker_trait + 'static,
                    #(#where_bounds),*
                {
                    type Key = rewrite_core::NodeId;
                    type Value = #value_type;

                    fn execute(db: &rewrite_core::Database, node: rewrite_core::NodeId, ctx: &mut rewrite_core::DependencyContext) -> Self::Value {
                        match Variant::to_value() {
                            #(#match_arms)*
                        }
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}
#[proc_macro_derive(Query, attributes(query, value_type, params))]
pub fn derive_query(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match &input.data {
        Data::Struct(_) => derive_query_for_struct(input),
        Data::Enum(data_enum) => {
            let data_enum_clone = data_enum.clone();
            derive_query_for_enum(input, &data_enum_clone)
        }
        _ => panic!("Query can only be derived for structs or enums"),
    }
}

fn derive_query_for_struct(input: DeriveInput) -> TokenStream {
    let struct_name = &input.ident;

    let value_type = input
        .attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("value_type") {
                let Meta::List(meta_list) = &attr.meta else {
                    panic!("value_type attribute must be in the form #[value_type(TypeName)]");
                };
                let type_name: Type = meta_list.parse_args().expect("Expected type name");
                Some(type_name)
            } else {
                None
            }
        })
        .expect("Query derive requires a #[value_type(TypeName)] attribute");

    let query_fn = input
        .attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("query") {
                let Meta::List(meta_list) = &attr.meta else {
                    panic!("query attribute must be in the form #[query(function_name)]");
                };
                let fn_name: syn::Path = meta_list.parse_args().expect("Expected function name");
                Some(fn_name)
            } else {
                None
            }
        })
        .expect("Query derive requires a #[query(function_name)] attribute");

    let param_traits = input.attrs.iter().find_map(|attr| {
        if attr.path().is_ident("params") {
            let Meta::List(meta_list) = &attr.meta else {
                return None;
            };
            let traits = meta_list
                .parse_args_with(
                    syn::punctuated::Punctuated::<Type, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            Some(traits.into_iter().collect::<Vec<_>>())
        } else {
            None
        }
    });

    let expanded = match param_traits.as_deref() {
        None => {
            quote! {
                impl rewrite_core::Query for #struct_name {
                    type Key = rewrite_core::NodeId;
                    type Value = #value_type;

                    fn execute(db: &rewrite_core::Database, node: rewrite_core::NodeId, ctx: &mut rewrite_core::DependencyContext) -> Self::Value {
                        let mut scoped = rewrite_core::ScopedDb::new(db, node, ctx);
                        #query_fn(&mut scoped)
                    }
                }
            }
        }
        Some(params) => {
            let param_count = params.len();
            let param_names: Vec<_> = (1..=param_count)
                .map(|i| syn::Ident::new(&format!("Param{}", i), struct_name.span()))
                .collect();

            let param_values: Vec<_> = param_names
                .iter()
                .map(|p| quote! { #p::to_value() })
                .collect();

            let where_bounds = param_names
                .iter()
                .zip(params.iter())
                .map(|(name, trait_bound)| {
                    quote! { #name: #trait_bound + 'static }
                });

            quote! {
                impl<#(#param_names),*> rewrite_core::Query for #struct_name
                where
                    #(#where_bounds),*
                {
                    type Key = rewrite_core::NodeId;
                    type Value = #value_type;

                    fn execute(db: &rewrite_core::Database, node: rewrite_core::NodeId, ctx: &mut rewrite_core::DependencyContext) -> Self::Value {
                        let mut scoped = rewrite_core::ScopedDb::new(db, node, ctx);
                        #query_fn(&mut scoped, #(#param_values),*)
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn derive_query_for_enum(input: DeriveInput, data_enum: &syn::DataEnum) -> TokenStream {
    let value_type = input
        .attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("value_type") {
                let Meta::List(meta_list) = &attr.meta else {
                    panic!("value_type attribute must be in the form #[value_type(TypeName)]");
                };
                let type_name: Type = meta_list.parse_args().expect("Expected type name");
                Some(type_name)
            } else {
                None
            }
        })
        .expect("Query derive requires a #[value_type(TypeName)] attribute on the enum");

    let mut generated_items = Vec::new();

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;

        if !matches!(variant.fields, Fields::Unit) {
            panic!("Query variants must be unit variants");
        }

        let query_fn = variant
            .attrs
            .iter()
            .find_map(|attr| {
                if attr.path().is_ident("query") {
                    let Meta::List(meta_list) = &attr.meta else {
                        panic!("query attribute must be in the form #[query(function_name)]");
                    };
                    let fn_name: syn::Path =
                        meta_list.parse_args().expect("Expected function name");
                    Some(fn_name)
                } else {
                    None
                }
            })
            .expect(&format!(
                "Variant {} must have a #[query(function_name)] attribute",
                variant_name
            ));

        let param_traits = variant.attrs.iter().find_map(|attr| {
            if attr.path().is_ident("params") {
                let Meta::List(meta_list) = &attr.meta else {
                    return None;
                };
                let traits = meta_list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<Type, syn::Token![,]>::parse_terminated,
                    )
                    .ok()?;
                Some(traits.into_iter().collect::<Vec<_>>())
            } else {
                None
            }
        });

        match param_traits.as_deref() {
            None => {
                let struct_name =
                    syn::Ident::new(&format!("{}Query", variant_name), variant_name.span());
                generated_items.push(quote! {
                    pub struct #struct_name;

                    impl rewrite_core::Query for #struct_name {
                        type Key = rewrite_core::NodeId;
                        type Value = #value_type;

                        fn execute(db: &rewrite_core::Database, node: rewrite_core::NodeId, ctx: &mut rewrite_core::DependencyContext) -> Self::Value {
                            let mut scoped = rewrite_core::ScopedDb::new(db, node, ctx);
                            #query_fn(&mut scoped)
                        }
                    }
                });
            }
            Some([trait_bound]) => {
                let struct_name =
                    syn::Ident::new(&format!("{}Query", variant_name), variant_name.span());
                generated_items.push(quote! {
                    pub struct #struct_name<Param1>(::std::marker::PhantomData<Param1>);

                    impl<Param1> rewrite_core::Query for #struct_name<Param1>
                    where
                        Param1: #trait_bound + 'static,
                    {
                        type Key = rewrite_core::NodeId;
                        type Value = #value_type;

                        fn execute(db: &rewrite_core::Database, node: rewrite_core::NodeId, ctx: &mut rewrite_core::DependencyContext) -> Self::Value {
                            let mut scoped = rewrite_core::ScopedDb::new(db, node, ctx);
                            #query_fn(&mut scoped, Param1::to_value())
                        }
                    }
                });
            }
            Some([trait_bound1, trait_bound2]) => {
                let struct_name =
                    syn::Ident::new(&format!("{}Query", variant_name), variant_name.span());
                generated_items.push(quote! {
                    pub struct #struct_name<Param1, Param2>(::std::marker::PhantomData<(Param1, Param2)>);

                    impl<Param1, Param2> rewrite_core::Query for #struct_name<Param1, Param2>
                    where
                        Param1: #trait_bound1 + 'static,
                        Param2: #trait_bound2 + 'static,
                    {
                        type Key = rewrite_core::NodeId;
                        type Value = #value_type;

                        fn execute(db: &rewrite_core::Database, node: rewrite_core::NodeId, ctx: &mut rewrite_core::DependencyContext) -> Self::Value {
                            let mut scoped = rewrite_core::ScopedDb::new(db, node, ctx);
                            #query_fn(&mut scoped, Param1::to_value(), Param2::to_value())
                        }
                    }
                });
            }
            _ => panic!("Only 0, 1, or 2 parameters are supported"),
        }
    }

    let expanded = quote! {
        #(#generated_items)*
    };

    TokenStream::from(expanded)
}
