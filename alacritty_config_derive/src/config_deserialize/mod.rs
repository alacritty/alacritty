use proc_macro::TokenStream;
use syn::{Data, DataStruct, DeriveInput, Error, Fields, parse_macro_input};

/// Error if the derive was used on an unsupported type.
const UNSUPPORTED_ERROR: &str = "ConfigDeserialize must be used on an enum or struct with fields";

mod de_enum;
mod de_struct;

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match input.data {
        Data::Struct(DataStruct { fields: Fields::Named(fields), .. }) => {
            de_struct::derive_deserialize(input.ident, input.generics, fields.named)
        },
        Data::Enum(data_enum) => {
            de_enum::derive_deserialize(input.ident, input.generics, data_enum)
        },
        _ => Error::new(input.ident.span(), UNSUPPORTED_ERROR).to_compile_error().into(),
    }
}
