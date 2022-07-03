//! The macros lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub, unused_crate_dependencies)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use proc_macro::{TokenStream};
use proc_macro2::{ Span};
use quote::quote;
use syn::{parse_macro_input, Type, Pat, AttributeArgs,  DeriveInput, Ident, ItemFn, Meta, NestedMeta, ReturnType};

mod shared;
use shared::*;
mod extract;

/// `fn_handler` is a pro macro to help create `Handler` from function easily.
///
/// `Handler` is a trait, `fn_handler` will convert you `fn` to a struct, and then implement `Handler`.
///
/// ```ignore
/// #[async_trait]
/// pub trait Handler: Send + Sync + 'static {
///     async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl);
/// }
/// ```
///
/// After use `fn_handler`, you don't need to care arguments' order, omit unused arguments:
///
/// ```ignore
/// #[fn_handler]
/// async fn hello_world() -> &'static str {
///     "Hello World"
/// }
/// ```
#[proc_macro_attribute]
pub fn fn_handler(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(input as ItemFn);
    let attrs = &item_fn.attrs;
    let vis = &item_fn.vis;
    let sig = &mut item_fn.sig;
    if sig.inputs.len() > 4 {
        return syn::Error::new_spanned(sig.fn_token, "too many args in handle function")
            .to_compile_error()
            .into();
    }
    // if sig.asyncness.is_none() {
    //     return syn::Error::new_spanned(sig.fn_token, "only async fn is supported")
    //         .to_compile_error()
    //         .into();
    //     // let ts: TokenStream = quote! {async}.into();
    //     // $sig.asyncness = Some(parse_macro_input!(ts as syn::token::Async))
    // }

    let body = &item_fn.block;
    let name = &sig.ident;
    let docs = item_fn
        .attrs
        .iter()
        .filter(|attr| attr.path.is_ident("doc"))
        .cloned()
        .collect::<Vec<_>>();

    let args: AttributeArgs = parse_macro_input!(args as AttributeArgs);
    let mut internal = false;
    for arg in args {
        if matches!(arg,NestedMeta::Meta(Meta::Path(p)) if p.is_ident("internal")) {
            internal = true;
            break;
        }
    }

    let salvo = salvo_crate(internal);

    let mut extract_ts = Vec::with_capacity(sig.inputs.len());
    let mut call_args: Vec<Ident> = Vec::with_capacity(sig.inputs.len());
    for input in &sig.inputs {
        match parse_input_type(input) {
            InputType::Request(_pat) => {
                call_args.push( Ident::new("req", Span::call_site()));
            }
            InputType::Depot(_pat) => {
                call_args.push(Ident::new("depot", Span::call_site()));
            }
            InputType::Response(_pat) => {
                call_args.push(Ident::new("res", Span::call_site()));
            }
            InputType::FlowCtrl(_pat) => {
                call_args.push(Ident::new("ctrl", Span::call_site()));
            }
            InputType::Unknown => {
                return syn::Error::new_spanned(
                    &sig.inputs,
                    "the inputs parameters must be Request, Depot, Response or FlowCtrl",
                )
                .to_compile_error()
                .into()
            }
            InputType::NoReference(pat) => {
                if let (Pat::Ident(ident), Type::Path(ty)) = (&*pat.pat, &*pat.ty) {
                    call_args.push(ident.ident.clone());
                    // Maybe extractible type.
                    let id = &pat.pat;
                    let (ty, lcount) = shared::omit_type_path_lifetimes(ty);
                    if lcount > 1 {
                        return syn::Error::new_spanned(pat, "Only one lifetime is allowed for `Extractible` type.")
                        .to_compile_error().into();
                    }

                    extract_ts.push(quote!{
                        let #id: #ty = match req.extract().await {
                            Ok(data) => data,
                            Err(e) => {
                                println!("failed to extract data: {}", e);
                                res.set_status_error(#salvo::http::errors::StatusError::bad_request().with_detail(
                                    "Extract data failed."
                                ));
                                return;
                            }
                        };
                    });
                } else {
                    return syn::Error::new_spanned(pat, "Invalid param definition.")
                    .to_compile_error().into();
                }
            }
        }
    }

    let sdef = quote! {
        #(#docs)*
        #[allow(non_camel_case_types)]
        #[derive(Debug)]
        #vis struct #name;
        impl #name {
            #(#attrs)*
            #sig {
                #body
            }
        }
    };

    match sig.output {
        ReturnType::Default => {
            if sig.asyncness.is_none() {
                (quote! {
                    #sdef
                    #[async_trait]
                    impl #salvo::Handler for #name {
                        #[inline]
                        async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response, ctrl: &mut #salvo::routing::FlowCtrl) {
                            #(#extract_ts)*
                            Self::#name(#(#call_args),*)
                        }
                    }
                })
                .into()
            } else {
                (quote! {
                    #sdef
                    #[async_trait]
                    impl #salvo::Handler for #name {
                        #[inline]
                        async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response, ctrl: &mut #salvo::routing::FlowCtrl) {
                            #(#extract_ts)*
                            Self::#name(#(#call_args),*).await
                        }
                    }
                })
                .into()
            }
        }
        ReturnType::Type(_, _) => {
            if sig.asyncness.is_none() {
                (quote! {
                    #sdef
                    #[async_trait]
                    impl #salvo::Handler for #name {
                        #[inline]
                        async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response, ctrl: &mut #salvo::routing::FlowCtrl) {
                           
                            #salvo::Writer::write(Self::#name(#(#call_args),*), req, depot, res).await;
                        }
                    }
                })
                .into()
            } else {
                (quote! {
                    #sdef
                    #[async_trait]
                    impl #salvo::Handler for #name {
                        #[inline]
                        async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response, ctrl: &mut #salvo::routing::FlowCtrl) {
                            #(#extract_ts)*
                            #salvo::Writer::write(Self::#name(#(#call_args),*).await, req, depot, res).await;
                        }
                    }
                })
                .into()
            }
        }
    }
}

/// Generate code for extractible type.
#[proc_macro_derive(Extractible, attributes(extract))]
pub fn derive_extractible(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DeriveInput);
    match extract::generate(args) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
