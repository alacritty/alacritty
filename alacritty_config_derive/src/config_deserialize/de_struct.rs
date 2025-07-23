use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Error, Field, Generics, Ident, Type};

use crate::{Attr, GenericsStreams, MULTIPLE_FLATTEN_ERROR, serde_replace};

/// Use this crate's name as log target.
const LOG_TARGET: &str = env!("CARGO_PKG_NAME");

pub fn derive_deserialize<T>(
    ident: Ident,
    generics: Generics,
    fields: Punctuated<Field, T>,
) -> TokenStream {
    // Create all necessary tokens for the implementation.
    let GenericsStreams { unconstrained, constrained, phantoms } =
        crate::generics_streams(&generics.params);
    let FieldStreams { flatten, match_assignments } = fields_deserializer(&fields);
    let visitor = format_ident!("{}Visitor", ident);

    // Generate deserialization impl.
    let mut tokens = quote! {
        #[derive(Default)]
        #[allow(non_snake_case)]
        struct #visitor <#unconstrained> {
            #phantoms
        }

        impl <'de, #constrained> serde::de::Visitor<'de> for #visitor <#unconstrained> {
            type Value = #ident <#unconstrained>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a mapping")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut config = Self::Value::default();

                // Unused keys for flattening and warning.
                let mut unused = toml::Table::new();

                while let Some((key, value)) = map.next_entry::<String, toml::Value>()? {
                    match key.as_str() {
                        #match_assignments
                        _ => {
                            unused.insert(key, value);
                        },
                    }
                }

                #flatten

                // Warn about unused keys.
                for key in unused.keys() {
                    log::warn!(target: #LOG_TARGET, "Unused config key: {}", key);
                }

                Ok(config)
            }
        }

        impl <'de, #constrained> serde::Deserialize<'de> for #ident <#unconstrained> {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_map(#visitor :: default())
            }
        }
    };

    // Automatically implement [`alacritty_config::SerdeReplace`].
    tokens.extend(serde_replace::derive_recursive(ident, generics, fields));

    tokens.into()
}

// Token streams created from the fields in the struct.
#[derive(Default)]
struct FieldStreams {
    match_assignments: TokenStream2,
    flatten: TokenStream2,
}

/// Create the deserializers for match arms and flattened fields.
fn fields_deserializer<T>(fields: &Punctuated<Field, T>) -> FieldStreams {
    let mut field_streams = FieldStreams::default();

    // Create the deserialization stream for each field.
    for field in fields.iter() {
        if let Err(err) = field_deserializer(&mut field_streams, field) {
            field_streams.flatten = err.to_compile_error();
            return field_streams;
        }
    }

    field_streams
}

/// Append a single field deserializer to the stream.
fn field_deserializer(field_streams: &mut FieldStreams, field: &Field) -> Result<(), Error> {
    let ident = field.ident.as_ref().expect("unreachable tuple struct");
    let literal = ident.to_string();
    let mut literals = vec![literal.clone()];

    // Create default stream for deserializing fields.
    let mut match_assignment_stream = quote! {
        match serde::Deserialize::deserialize(value) {
            Ok(value) => config.#ident = value,
            Err(err) => {
                log::error!(
                    target: #LOG_TARGET,
                    "Config error: {}: {}",
                    #literal,
                    err.to_string().trim(),
                );
            },
        }
    };

    // Iterate over all #[config(...)] attributes.
    for attr in field.attrs.iter().filter(|attr| attr.path().is_ident("config")) {
        let parsed = match attr.parse_args::<Attr>() {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };

        match parsed.ident.as_str() {
            // Skip deserialization for `#[config(skip)]` fields.
            "skip" => return Ok(()),
            "flatten" => {
                // NOTE: Currently only a single instance of flatten is supported per struct
                // for complexity reasons.
                if !field_streams.flatten.is_empty() {
                    return Err(Error::new(attr.span(), MULTIPLE_FLATTEN_ERROR));
                }

                // Create the tokens to deserialize the flattened struct from the unused fields.
                field_streams.flatten.extend(quote! {
                    // Drain unused fields since they will be used for flattening.
                    let flattened = std::mem::replace(&mut unused, toml::Table::new());

                    config.#ident = serde::Deserialize::deserialize(flattened).unwrap_or_default();
                });
            },
            "deprecated" | "removed" => {
                // Construct deprecation/removal message with optional attribute override.
                let mut message = format!("Config warning: {} has been {}", literal, parsed.ident);
                if let Some(warning) = parsed.param {
                    message = format!("{}; {}", message, warning.value());
                }
                message.push_str("\nUse `alacritty migrate` to automatically resolve it");

                // Append stream to log deprecation/removal warning.
                match_assignment_stream.extend(quote! {
                    log::warn!(target: #LOG_TARGET, #message);
                });
            },
            // Add aliases to match pattern.
            "alias" => {
                if let Some(alias) = parsed.param {
                    literals.push(alias.value());
                }
            },
            _ => (),
        }
    }

    // Create token stream for deserializing "none" string into `Option<T>`.
    if let Type::Path(type_path) = &field.ty {
        if type_path.path.segments.iter().next_back().is_some_and(|s| s.ident == "Option") {
            match_assignment_stream = quote! {
                if value.as_str().is_some_and(|s| s.eq_ignore_ascii_case("none")) {
                    config.#ident = None;
                    continue;
                }
                #match_assignment_stream
            };
        }
    }

    // Create the token stream for deserialization and error handling.
    field_streams.match_assignments.extend(quote! {
        #(#literals)|* => { #match_assignment_stream },
    });

    Ok(())
}
