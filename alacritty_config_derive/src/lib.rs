use proc_macro::TokenStream;
use proc_macro2::{Literal as Literal2, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Data, DataStruct, DeriveInput, Error, Field, Fields, GenericParam, Type,
    TypeParam,
};

/// Error if the derive was used on an unsupported type.
const UNSUPPORTED_ERROR: &str = "ConfigDeserialize must be used on a struct with fields";

#[proc_macro_derive(ConfigDeserialize)]
pub fn derive_config_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Only accept this derive on non-unit structs.
    let fields = match input.data {
        Data::Struct(DataStruct { fields: Fields::Named(fields), .. }) => fields.named,
        _ => {
            return Error::new(input.ident.span(), UNSUPPORTED_ERROR).to_compile_error().into();
        },
    };

    // Create all necessary tokens for the implementation.
    let Generics { unconstrained, constrained, phantoms } = generics(input.generics.params);
    let fields_deserializer = fields_deserializer(fields);
    let visitor = format_ident!("{}Visitor", input.ident);
    let ident = input.ident;

    // Generate deserialization impl.
    let tokens = quote! {
        #[derive(Default)]
        #[allow(non_snake_case)]
        struct #visitor < #unconstrained > {
            #phantoms
        }

        impl<'de, #constrained> serde::de::Visitor<'de> for #visitor < #unconstrained > {
            type Value = #ident < #unconstrained >;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a mapping")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut config = Self::Value::default();

                while let Some((key, value)) = map.next_entry::<String, serde_yaml::Value>()? {
                    match key.as_str() {
                        #fields_deserializer
                        // NOTE: Unused variables could be logged here.
                        _ => (),
                    }
                }

                Ok(config)
            }
        }

        impl<'de, #constrained> serde::Deserialize<'de> for #ident < #unconstrained > {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_map(#visitor :: default())
            }
        }
    };

    tokens.into()
}

/// Create the deserialize match arm for each field.
///
/// The resulting code for one field might look like this:
///
/// ```rust
///     "field" => {
///         match serde::Deserialize::deserialize(value) {
///             Ok(value) => config.field = value,
///             Err(err) => {
///                 log::error!(target: env!("CARGO_PKG_NAME"), "Config error: {}", err);
///             },
///         }
///     },
/// ```
fn fields_deserializer<T>(fields: Punctuated<Field, T>) -> TokenStream2 {
    let mut fields_deserializer = TokenStream2::new();

    for field in fields.iter() {
        let ident = field.ident.as_ref().expect("unreachable tuple struct");

        // Create token stream for deserializing "none" string into `Option<T>`.
        let mut none_string_option = TokenStream2::new();
        if let Type::Path(type_path) = &field.ty {
            let segments = type_path.path.segments.iter();
            if segments.last().map_or(false, |segment| segment.ident.to_string() == "Option") {
                none_string_option = quote! {
                    if value.as_str().map_or(false, |s| s.eq_ignore_ascii_case("none")) {
                        config.#ident = None;
                        continue;
                    }
                };
            }
        }

        // Create token stream for deserialization and error handling.
        let lit = Literal2::string(&ident.to_string());
        fields_deserializer.extend(quote! {
            #lit => {
                #none_string_option

                match serde::Deserialize::deserialize(value) {
                    Ok(value) => config.#ident = value,
                    Err(err) => {
                        log::error!(target: env!("CARGO_PKG_NAME"), "Config error: {}", err);
                    },
                }
            },
        });
    }

    fields_deserializer
}

/// Storage for all necessary generics information.
#[derive(Default)]
struct Generics {
    unconstrained: TokenStream2,
    constrained: TokenStream2,
    phantoms: TokenStream2,
}

/// Create the necessary generics annotations.
///
/// This will create three different token streams, which might look like this:
///  - unconstrained: `T`
///  - constrained: `T: Default + Deserialize<'de>`
///  - phantoms: `T: PhantomData<T>,`
fn generics<T>(params: Punctuated<GenericParam, T>) -> Generics {
    let mut generics = Generics::default();

    for generic in params {
        // NOTE: Lifetimes and const params are not supported.
        if let GenericParam::Type(TypeParam { ident, .. }) = generic {
            generics.unconstrained.extend(quote!( #ident , ));
            generics.constrained.extend(quote! {
                #ident : Default + serde::Deserialize<'de> ,
            });
            generics.phantoms.extend(quote! {
                #ident : std::marker::PhantomData < #ident >,
            });
        }
    }

    generics
}
