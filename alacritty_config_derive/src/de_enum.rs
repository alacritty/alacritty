use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{DataEnum, Ident};

pub fn derive_deserialize(ident: Ident, data_enum: DataEnum) -> TokenStream {
    let visitor = format_ident!("{}Visitor", ident);

    // Create match arm streams and get a list with all available values.
    let mut match_arms_stream = TokenStream2::new();
    let mut available_values = String::from("one of ");
    for variant in data_enum.variants.iter().filter(|variant| {
        // Skip deserialization for `#[config(skip)]` fields.
        variant.attrs.iter().all(|attr| {
            !crate::path_ends_with(&attr.path, "config") || attr.tokens.to_string() != "(skip)"
        })
    }) {
        let variant_ident = &variant.ident;
        let variant_str = variant_ident.to_string();
        available_values = format!("{}`{}`, ", available_values, variant_str);

        let literal = variant_str.to_lowercase();

        match_arms_stream.extend(quote! {
            #literal => Ok(#ident :: #variant_ident),
        });
    }

    // Remove trailing `, ` from the last enum variant.
    available_values.truncate(available_values.len().saturating_sub(2));

    // Generate deserialization impl.
    let tokens = quote! {
        struct #visitor;
        impl<'de> serde::de::Visitor<'de> for #visitor {
            type Value = #ident;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(#available_values)
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match s.to_lowercase().as_str() {
                    #match_arms_stream
                    _ => Err(E::custom(
                            &format!("unknown variant `{}`, expected {}", s, #available_values)
                    )),
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for #ident {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_str(#visitor)
            }
        }
    };

    tokens.into()
}
