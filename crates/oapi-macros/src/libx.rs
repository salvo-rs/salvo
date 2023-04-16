//! The macros lib of salvo_oapi. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)] //, unused_crate_dependencies
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]
#![warn(rustdoc::broken_intra_doc_links)]

use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, AttributeArgs, DeriveInput, Item, ItemFn, ItemImpl, Meta, NestedMeta, Token};

// mod object;
mod endpoint;
mod shared;

// #[proc_macro_derive(Object, attributes(salvo_oapi))]
// pub fn derive_object(input: TokenStream) -> TokenStream {
//     let args = parse_macro_input!(input as DeriveInput);
//     match object::generate(args) {
//         Ok(stream) => stream.into(),
//         Err(err) => err.write_errors().into(),
//     }
// }

// #[proc_macro_derive(Enum, attributes(salvo_oapi))]
// pub fn derive_enum(input: TokenStream) -> TokenStream {
//     let args = parse_macro_input!(input as DeriveInput);
//     match r#enum::generate(args) {
//         Ok(stream) => stream.into(),
//         Err(err) => err.write_errors().into(),
//     }
// }

// #[proc_macro_derive(Union, attributes(salvo_oapi))]
// pub fn derive_union(input: TokenStream) -> TokenStream {
//     let args = parse_macro_input!(input as DeriveInput);
//     match union::generate(args) {
//         Ok(stream) => stream.into(),
//         Err(err) => err.write_errors().into(),
//     }
// }

#[proc_macro_attribute]
pub fn endpoint(args: TokenStream, input: TokenStream) -> TokenStream {
    let internal = !args.is_empty();
    let item = parse_macro_input!(input as Item);
    let stream = match endpoint::generate(internal, item) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    };
    // println!("{}", stream);
    stream
}