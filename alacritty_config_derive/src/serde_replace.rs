use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    Data, DataStruct, DeriveInput, Error, Field, Fields, Generics, Ident, parse_macro_input,
};

use crate::{Attr, GenericsStreams, MULTIPLE_FLATTEN_ERROR};

/// Error if the derive was used on an unsupported type.
const UNSUPPORTED_ERROR: &str = "SerdeReplace must be used on a tuple struct";

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match input.data {
        Data::Struct(DataStruct { fields: Fields::Unnamed(_), .. }) | Data::Enum(_) => {
            derive_direct(input.ident, input.generics).into()
        },
        Data::Struct(DataStruct { fields: Fields::Named(fields), .. }) => {
            derive_recursive(input.ident, input.generics, fields.named).into()
        },
        _ => Error::new(input.ident.span(), UNSUPPORTED_ERROR).to_compile_error().into(),
    }
}

pub fn derive_direct(ident: Ident, generics: Generics) -> TokenStream2 {
    quote! {
        impl <#generics> alacritty_config::SerdeReplace for #ident <#generics> {
            fn replace(&mut self, value: toml::Value) -> Result<(), Box<dyn std::error::Error>> {
                *self = serde::Deserialize::deserialize(value)?;

                Ok(())
            }
        }
    }
}

pub fn derive_recursive<T>(
    ident: Ident,
    generics: Generics,
    fields: Punctuated<Field, T>,
) -> TokenStream2 {
    let GenericsStreams { unconstrained, constrained, .. } =
        crate::generics_streams(&generics.params);
    let replace_arms = match match_arms(&fields) {
        Err(e) => return e.to_compile_error(),
        Ok(replace_arms) => replace_arms,
    };

    quote! {
        #[allow(clippy::extra_unused_lifetimes)]
        impl <'de, #constrained> alacritty_config::SerdeReplace for #ident <#unconstrained> {
            fn replace(&mut self, value: toml::Value) -> Result<(), Box<dyn std::error::Error>> {
                match value.as_table() {
                    Some(table) => {
                        for (field, next_value) in table {
                            let next_value = next_value.clone();
                            let value = value.clone();

                            match field.as_str() {
                                #replace_arms
                                _ => {
                                    let error = format!("Field \"{}\" does not exist", field);
                                    return Err(error.into());
                                },
                            }
                        }
                    },
                    None => *self = serde::Deserialize::deserialize(value)?,
                }

                Ok(())
            }
        }
    }
}

/// Create SerdeReplace recursive match arms.
fn match_arms<T>(fields: &Punctuated<Field, T>) -> Result<TokenStream2, syn::Error> {
    let mut stream = TokenStream2::default();
    let mut flattened_arm = None;

    // Create arm for each field.
    for field in fields {
        let ident = field.ident.as_ref().expect("unreachable tuple struct");
        let literal = ident.to_string();

        // Check if #[config(flattened)] attribute is present.
        let flatten = field
            .attrs
            .iter()
            .filter(|attr| (*attr).path().is_ident("config"))
            .filter_map(|attr| attr.parse_args::<Attr>().ok())
            .any(|parsed| parsed.ident.as_str() == "flatten");

        if flatten && flattened_arm.is_some() {
            return Err(Error::new(ident.span(), MULTIPLE_FLATTEN_ERROR));
        } else if flatten {
            flattened_arm = Some(quote! {
                _ => alacritty_config::SerdeReplace::replace(&mut self.#ident, value)?,
            });
        } else {
            // Extract all `#[config(alias = "...")]` attribute values.
            let aliases = field
                .attrs
                .iter()
                .filter(|attr| (*attr).path().is_ident("config"))
                .filter_map(|attr| attr.parse_args::<Attr>().ok())
                .filter(|parsed| parsed.ident.as_str() == "alias")
                .map(|parsed| {
                    let value = parsed
                        .param
                        .ok_or_else(|| format!("Field \"{ident}\" has no alias value"))?
                        .value();

                    if value.trim().is_empty() {
                        return Err(format!("Field \"{ident}\" has an empty alias value"));
                    }

                    Ok(value)
                })
                .collect::<Result<Vec<String>, String>>()
                .map_err(|msg| Error::new(ident.span(), msg))?;

            stream.extend(quote! {
                    #(#aliases)|* | #literal => alacritty_config::SerdeReplace::replace(&mut
            self.#ident, next_value)?,         });
        }
    }

    // Add the flattened catch-all as last match arm.
    if let Some(flattened_arm) = flattened_arm.take() {
        stream.extend(flattened_arm);
    }

    Ok(stream)
}
