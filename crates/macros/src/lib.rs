//! Procedural macros for the Salvo web framework.
//!
//! This crate is normally used through the re-exports from `salvo` or
//! `salvo_core::prelude`. It provides the `#[handler]` attribute for turning
//! functions or `impl` blocks into Salvo handlers, and the `Extractible` derive
//! for request extraction metadata.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(clippy::unwrap_used))]

use proc_macro::TokenStream;
use syn::{DeriveInput, Item, parse_macro_input};

mod attribute;
mod extract;
mod handler;
mod shared;

pub(crate) use salvo_serde_util as serde_util;
use shared::*;

/// Converts a function or `impl` block into a Salvo `Handler`.
///
/// On a function, `#[handler]` generates a zero-sized handler type with the
/// same name as the function and implements `salvo::Handler` for it. Salvo
/// injects any supported arguments by type, so handlers can list only the
/// values they need and in any order:
///
/// - `&mut Request`
/// - `&mut Depot`
/// - `&mut Response`
/// - `&mut FlowCtrl`
/// - extractible request data
///
/// A return value that implements Salvo's writer traits is written to the
/// response automatically.
///
/// On an `impl` block, the method named `handle` is used as the implementation
/// of `Handler` for the implementing type.
///
/// # Examples
///
/// ```rust,ignore
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn hello() -> &'static str {
///     "Hello, world!"
/// }
/// ```
///
/// ```rust,ignore
/// use salvo_core::prelude::*;
///
/// struct Health;
///
/// #[handler]
/// impl Health {
///     fn handle() -> &'static str {
///         "ok"
///     }
/// }
/// ```
///
/// See `salvo_core::handler` for the full handler guide.
#[proc_macro_attribute]
pub fn handler(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    match handler::generate(item) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Implements `salvo::extract::Extractible` for a request data struct.
///
/// `Extractible` describes where each field should be read from and then uses
/// Salvo's request deserialization support to build the struct in a handler.
/// The derive is intended for named-field structs that also implement
/// `serde::Deserialize`.
///
/// Container attributes:
///
/// - `#[salvo(extract(default_source(from = "query")))]` sets a fallback source for fields without
///   an explicit source.
/// - `from` accepts `param`, `query`, `header`, `body`, or `depot`.
/// - `parse` accepts `smart`, `json`, or `multimap`.
/// - `#[salvo(extract(rename_all = "camelCase"))]` renames fields with the same case rules used by
///   serde.
///
/// Field attributes:
///
/// - `#[salvo(extract(source(from = "param")))]` reads a field from a specific request source.
/// - `#[salvo(extract(rename = "userId"))]` overrides the request field name.
/// - `#[salvo(extract(alias = "uid"))]` accepts an alternate field name.
/// - `#[salvo(extract(flatten))]` flattens another `Extractible` struct into the same request
///   metadata.
///
/// `#[serde(rename)]`, `#[serde(rename_all)]`, `#[serde(alias)]`, and serde
/// defaults are honored where they map to Salvo extraction metadata.
/// `#[serde(flatten)]` is intentionally rejected; use
/// `#[salvo(extract(flatten))]` instead.
///
/// # Example
///
/// ```rust,ignore
/// use salvo_core::prelude::*;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Extractible)]
/// #[salvo(extract(default_source(from = "query")))]
/// struct Search<'a> {
///     #[salvo(extract(source(from = "param")))]
///     id: i64,
///     term: &'a str,
/// }
///
/// #[handler]
/// async fn search(req: &mut Request, depot: &mut Depot) {
///     let value: Search<'_> = req.extract(depot).await.unwrap();
///     let _ = value;
/// }
/// ```
#[proc_macro_derive(Extractible, attributes(salvo))]
pub fn derive_extractible(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DeriveInput);
    match extract::generate(args) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse2;

    use super::*;

    #[test]
    fn test_handler_for_fn() {
        let input = quote! {
            #[handler]
            async fn hello(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
                res.render_plain_text("Hello World");
            }
        };
        let item = parse2(input).unwrap();
        assert_eq!(
            handler::generate(item).unwrap().to_string(),
            quote! {
                #[allow(non_camel_case_types)]
                #[derive(Debug)]
                struct hello;
                impl hello {
                    async fn hello(req: &mut Request,depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
                        {
                            res.render_plain_text("Hello World");
                        }
                    }
                }
                #[salvo::async_trait]
                impl salvo::Handler for hello {
                    async fn handle(
                        &self,
                        __macro_gen_req: &mut salvo::Request,
                        __macro_gen_depot: &mut salvo::Depot,
                        __macro_gen_res: &mut salvo::Response,
                        __macro_gen_ctrl: &mut salvo::FlowCtrl
                    ) {
                        Self::hello(__macro_gen_req, __macro_gen_depot, __macro_gen_res, __macro_gen_ctrl).await
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn test_handler_for_fn_return_result() {
        let input = quote! {
            #[handler]
            async fn hello(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) -> Result<(), Error> {
                Ok(())
            }
        };
        let item = parse2(input).unwrap();
        assert_eq!(
            handler::generate(item).unwrap().to_string(),
            quote!{
                #[allow(non_camel_case_types)]
                #[derive(Debug)]
                struct hello;
                impl hello {
                    async fn hello(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl
                    ) -> Result<(), Error> {
                        {Ok(())}
                    }
                }
                #[salvo::async_trait]
                impl salvo::Handler for hello {
                    async fn handle(
                        &self,
                        __macro_gen_req: &mut salvo::Request,
                        __macro_gen_depot: &mut salvo::Depot,
                        __macro_gen_res: &mut salvo::Response,
                        __macro_gen_ctrl: &mut salvo::FlowCtrl
                    ) {
                        salvo::Writer::write(Self::hello(__macro_gen_req, __macro_gen_depot, __macro_gen_res, __macro_gen_ctrl).await, __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn test_handler_for_impl() {
        let input = quote! {
            #[handler]
            impl Hello {
                fn handle(req: &mut Request, depot: &mut Depot, res: &mut Response) {
                    res.render_plain_text("Hello World");
                }
            }
        };
        let item = parse2(input).unwrap();
        assert_eq!(
            handler::generate(item).unwrap().to_string(),
            quote! {
                #[handler]
                impl Hello {
                    fn handle(req: &mut Request, depot: &mut Depot, res: &mut Response) {
                        res.render_plain_text("Hello World");
                    }
                }
                #[salvo::async_trait]
                impl salvo::Handler for Hello {
                    async fn handle(
                        &self,
                        __macro_gen_req: &mut salvo::Request,
                        __macro_gen_depot: &mut salvo::Depot,
                        __macro_gen_res: &mut salvo::Response,
                        __macro_gen_ctrl: &mut salvo::FlowCtrl
                    ) {
                        Self::handle(__macro_gen_req, __macro_gen_depot, __macro_gen_res)
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn test_extract_simple() {
        let input = quote! {
            #[salvo(extract(default_source(from = "body")))]
            struct BadMan<'a> {
                #[salvo(extract(source(from = "query")))]
                id: i64,
                username: String,
            }
        };
        let item = parse2(input).unwrap();
        assert_eq!(
            extract::generate(item).unwrap().to_string(),
            quote!{
                impl<'__macro_gen_ex: 'a, 'a> salvo::extract::Extractible<'__macro_gen_ex> for BadMan<'a> {
                    fn metadata() -> &'static salvo::extract::Metadata {
                        static METADATA: ::std::sync::OnceLock<salvo::extract::Metadata> = ::std::sync::OnceLock::new();
                        METADATA.get_or_init(|| {
                            let mut metadata = salvo::extract::Metadata::new("BadMan");
                            metadata = metadata.add_default_source(salvo::extract::metadata::Source::new(
                                salvo::extract::metadata::SourceFrom::Body,
                                salvo::extract::metadata::SourceParser::Smart
                            ));
                            let mut field = salvo::extract::metadata::Field::new("id");
                            field = field.add_source(salvo::extract::metadata::Source::new(
                                salvo::extract::metadata::SourceFrom::Query,
                                salvo::extract::metadata::SourceParser::Smart
                            ));
                            metadata = metadata.add_field(field);
                            let mut field = salvo::extract::metadata::Field::new("username");
                            metadata = metadata.add_field(field);
                            metadata
                        })
                    }
                    #[allow(refining_impl_trait)]
                    async fn extract(req: &'__macro_gen_ex mut salvo::http::Request, depot: &'__macro_gen_ex mut salvo::Depot) -> ::std::result::Result<Self, salvo::http::ParseError>
                    where
                        Self: Sized {
                        salvo::serde::from_request(req, depot, Self::metadata()).await
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn test_extract_with_lifetime() {
        let input = quote! {
            #[salvo(extract(
                default_source(from = "query"),
                default_source(from = "param"),
                default_source(from = "body")
            ))]
            struct BadMan<'a> {
                id: i64,
                username: String,
                first_name: &'a str,
                last_name: String,
                lovers: Vec<String>,
            }
        };
        let item = parse2(input).unwrap();
        assert_eq!(
            extract::generate(item).unwrap().to_string(),
            quote!{
                impl<'__macro_gen_ex: 'a, 'a> salvo::extract::Extractible<'__macro_gen_ex> for BadMan<'a> {
                    fn metadata() -> &'static salvo::extract::Metadata {
                        static METADATA: ::std::sync::OnceLock<salvo::extract::Metadata> = ::std::sync::OnceLock::new();
                        METADATA.get_or_init(|| {
                            let mut metadata = salvo::extract::Metadata::new("BadMan");
                            metadata = metadata.add_default_source(salvo::extract::metadata::Source::new(
                                salvo::extract::metadata::SourceFrom::Query,
                                salvo::extract::metadata::SourceParser::Smart
                            ));
                            metadata = metadata.add_default_source(salvo::extract::metadata::Source::new(
                                salvo::extract::metadata::SourceFrom::Param,
                                salvo::extract::metadata::SourceParser::Smart
                            ));
                            metadata = metadata.add_default_source(salvo::extract::metadata::Source::new(
                                salvo::extract::metadata::SourceFrom::Body,
                                salvo::extract::metadata::SourceParser::Smart
                            ));
                            let mut field = salvo::extract::metadata::Field::new("id");
                            metadata = metadata.add_field(field);
                            let mut field = salvo::extract::metadata::Field::new("username");
                            metadata = metadata.add_field(field);
                            let mut field = salvo::extract::metadata::Field::new("first_name");
                            metadata = metadata.add_field(field);
                            let mut field = salvo::extract::metadata::Field::new("last_name");
                            metadata = metadata.add_field(field);
                            let mut field = salvo::extract::metadata::Field::new("lovers");
                            metadata = metadata.add_field(field);
                            metadata
                        })
                    }
                    #[allow(refining_impl_trait)]
                    async fn extract(req: &'__macro_gen_ex mut salvo::http::Request, depot: &'__macro_gen_ex mut salvo::Depot) -> ::std::result::Result<Self, salvo::http::ParseError>
                    where
                        Self: Sized {
                        salvo::serde::from_request(req, depot, Self::metadata()).await
                    }
                }
            }
            .to_string()
        );
    }
}
