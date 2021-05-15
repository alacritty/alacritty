use proc_macro::TokenStream;
use syn::{parse_macro_input, Data, DataStruct, DeriveInput, Error, Fields, Path};

mod de_enum;
mod de_struct;

/// Error if the derive was used on an unsupported type.
const UNSUPPORTED_ERROR: &str = "ConfigDeserialize must be used on a struct with fields";

#[proc_macro_derive(ConfigDeserialize, attributes(config))]
pub fn derive_config_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match input.data {
        Data::Struct(DataStruct { fields: Fields::Named(fields), .. }) => {
            de_struct::derive_deserialize(input.ident, input.generics, fields.named)
        },
        Data::Enum(data_enum) => de_enum::derive_deserialize(input.ident, data_enum),
        _ => Error::new(input.ident.span(), UNSUPPORTED_ERROR).to_compile_error().into(),
    }
}

/// Verify that a token path ends with a specific segment.
pub(crate) fn path_ends_with(path: &Path, segment: &str) -> bool {
    let segments = path.segments.iter();
    segments.last().map_or(false, |s| s.ident == segment)
}
