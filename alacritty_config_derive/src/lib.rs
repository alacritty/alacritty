use proc_macro::TokenStream;
use proc_macro2::{Literal as Literal2, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Data, DataStruct, DeriveInput, Field, Fields, GenericParam, TypeParam, Error,
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
///     "field" => config.field = serde::Deserialize::deserialize(value).unwrap_or_default(),
/// ```
fn fields_deserializer<T>(fields: Punctuated<Field, T>) -> TokenStream2 {
    let mut fields_deserializer = TokenStream2::new();

    for field in fields.iter().filter_map(|field| field.ident.as_ref()) {
        let lit = Literal2::string(&field.to_string());
        fields_deserializer = quote! {
            #fields_deserializer
            #lit => config.#field = serde::Deserialize::deserialize(value).unwrap_or_default(),
        };
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
        match generic {
            GenericParam::Type(TypeParam { ident, .. }) => {
                generics.unconstrained.extend(quote!( #ident , ));
                generics.constrained.extend(quote! {
                    #ident : Default + serde::Deserialize<'de> ,
                });
                generics.phantoms.extend(quote! {
                    #ident : std::marker::PhantomData < #ident >,
                });
            },
            // NOTE: Lifetimes and const params are not supported.
            _ => (),
        }
    }

    generics
}
