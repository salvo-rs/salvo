//! The macros lib of Salvo web framework.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use proc_macro::TokenStream;
use syn::{DeriveInput, Item, parse_macro_input};

mod attribute;
mod extract;
mod handler;
mod shared;

pub(crate) use salvo_serde_util as serde_util;
use shared::*;

/// `handler` is a macro to help create `Handler` from function or impl block easily.
///
/// `Handler` is a trait, if `#[handler]` applied to `fn`,  `fn` will converted to a struct, and then implement `Handler`,
/// after use `handler`, you don't need to care arguments' order, omit unused arguments.
///
/// View `salvo_core::handler` for more details.
#[proc_macro_attribute]
pub fn handler(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    match handler::generate(item) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Generate code for extractible type.
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
                    async fn extract(req: &'__macro_gen_ex mut salvo::http::Request) -> Result<Self, salvo::http::ParseError>
                    where
                        Self: Sized {
                        salvo::serde::from_request(req, Self::metadata()).await
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
                    async fn extract(req: &'__macro_gen_ex mut salvo::http::Request) -> Result<Self, salvo::http::ParseError>
                    where
                        Self: Sized {
                        salvo::serde::from_request(req, Self::metadata()).await
                    }
                }
            }
            .to_string()
        );
    }
}
