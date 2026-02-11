use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

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
