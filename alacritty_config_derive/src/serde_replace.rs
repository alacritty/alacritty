use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Data, DataStruct, DeriveInput, Error, Field, Fields, Generics, Ident,
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
            fn replace(&mut self, key: &str, value: serde_yaml::Value) -> Result<(), Box<dyn std::error::Error>> {
                if !key.is_empty() {
                    let error = format!("Fields \"{}\" do not exist", key);
                    return Err(error.into());
                }
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
    let replace_arms = match_arms(&fields);

    quote! {
        #[allow(clippy::extra_unused_lifetimes)]
        impl <'de, #constrained> alacritty_config::SerdeReplace for #ident <#unconstrained> {
            fn replace(&mut self, key: &str, value: serde_yaml::Value) -> Result<(), Box<dyn std::error::Error>> {
                if key.is_empty() {
                    *self = serde::Deserialize::deserialize(value)?;
                    return Ok(());
                }

                let (field, next_key) = key.split_once('.').unwrap_or((key, ""));
                match field {
                    #replace_arms
                    _ => {
                        let error = format!("Field \"{}\" does not exist", field);
                        return Err(error.into());
                    },
                }

                Ok(())
            }
        }
    }
}

/// Create SerdeReplace recursive match arms.
fn match_arms<T>(fields: &Punctuated<Field, T>) -> TokenStream2 {
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
            .filter_map(|attr| attr.parse_args::<Attr>().ok())
            .any(|parsed| parsed.ident.as_str() == "flatten");

        if flatten && flattened_arm.is_some() {
            return Error::new(ident.span(), MULTIPLE_FLATTEN_ERROR).to_compile_error();
        } else if flatten {
            flattened_arm = Some(quote! {
                _ => alacritty_config::SerdeReplace::replace(&mut self.#ident, key, value)?,
            });
        } else {
            stream.extend(quote! {
                #literal => alacritty_config::SerdeReplace::replace(&mut self.#ident, next_key, value)?,
            });
        }
    }

    // Add the flattened catch-all as last match arm.
    if let Some(flattened_arm) = flattened_arm.take() {
        stream.extend(flattened_arm);
    }

    stream
}
