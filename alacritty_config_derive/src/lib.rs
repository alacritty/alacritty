#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]
#![cfg_attr(clippy, deny(warnings))]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{self, Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{GenericParam, Ident, LitStr, Token, TypeParam};

mod config_deserialize;
mod serde_replace;

/// Error message when attempting to flatten multiple fields.
pub(crate) const MULTIPLE_FLATTEN_ERROR: &str =
    "At most one instance of #[config(flatten)] is supported";

#[proc_macro_derive(ConfigDeserialize, attributes(config))]
pub fn derive_config_deserialize(input: TokenStream) -> TokenStream {
    config_deserialize::derive(input)
}

#[proc_macro_derive(SerdeReplace)]
pub fn derive_serde_replace(input: TokenStream) -> TokenStream {
    serde_replace::derive(input)
}

/// Storage for all necessary generics information.
#[derive(Default)]
struct GenericsStreams {
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
pub(crate) fn generics_streams<T>(params: &Punctuated<GenericParam, T>) -> GenericsStreams {
    let mut generics = GenericsStreams::default();

    for generic in params {
        // NOTE: Lifetimes and const params are not supported.
        if let GenericParam::Type(TypeParam { ident, .. }) = generic {
            generics.unconstrained.extend(quote!( #ident , ));
            generics.constrained.extend(quote! {
                #ident : Default + serde::Deserialize<'de> + alacritty_config::SerdeReplace,
            });
            generics.phantoms.extend(quote! {
                #ident : std::marker::PhantomData < #ident >,
            });
        }
    }

    generics
}

/// Field attribute.
pub(crate) struct Attr {
    ident: String,
    param: Option<LitStr>,
}

impl Parse for Attr {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        let ident = input.parse::<Ident>()?.to_string();
        let param = input.parse::<Token![=]>().and_then(|_| input.parse()).ok();
        Ok(Self { ident, param })
    }
}
