//! The macros lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/images/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub, unused_crate_dependencies)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use proc_quote::quote;
use syn::punctuated::Punctuated;
use syn::{Ident, ReturnType};

enum InputType {
    Request,
    Depot,
    Response,
    FlowCtrl,
    UnKnow,
    NoReferenceArg,
}

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
pub fn fn_handler(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = syn::parse_macro_input!(input as syn::ItemFn);
    let attrs = &item_fn.attrs;
    let vis = &item_fn.vis;
    let sig = &mut item_fn.sig;
    if sig.inputs.len() > 4 {
        return syn::Error::new_spanned(sig.fn_token, "too many args in handle function")
            .to_compile_error()
            .into();
    }
    if sig.asyncness.is_none() {
        return syn::Error::new_spanned(sig.fn_token, "only async fn is supported")
            .to_compile_error()
            .into();
        // let ts: TokenStream = quote! {async}.into();
        // $sig.asyncness = Some(syn::parse_macro_input!(ts as syn::token::Async))
    }

    let body = &item_fn.block;
    let name = &sig.ident;
    let docs = item_fn
        .attrs
        .iter()
        .filter(|attr| attr.path.is_ident("doc"))
        .cloned()
        .collect::<Vec<_>>();

    let salvo = salvo_crate();
    
    let inputs = std::mem::replace(&mut sig.inputs, Punctuated::new());
    let mut req_ts = None;
    let mut depot_ts = None;
    let mut res_ts = None;
    let mut ctrl_ts = None;
    for input in inputs {
        match parse_input_type(&input) {
            InputType::Request => {
                req_ts = Some(input);
            }
            InputType::Depot => {
                depot_ts = Some(input);
            }
            InputType::Response => {
                res_ts = Some(input);
            }
            InputType::FlowCtrl => {
                ctrl_ts = Some(input);
            }
            InputType::UnKnow => {
                return syn::Error::new_spanned(
                    &sig.inputs,
                    "the inputs parameters must be Request, Depot, Response or FlowCtrl",
                )
                .to_compile_error()
                .into()
            }
            InputType::NoReferenceArg => {
                return syn::Error::new_spanned(
                    &sig.inputs,
                    "the inputs parameters must be mutable reference Request, Depot, Response or FlowCtrl",
                )
                .to_compile_error()
                .into()
            }
        }
    }
    if let Some(ts) = req_ts {
        sig.inputs.push(ts);
    } else {
        let ts: TokenStream = quote! {_req: &mut #salvo::Request}.into();
        sig.inputs.push(syn::parse_macro_input!(ts as syn::FnArg));
    }
    if let Some(ts) = depot_ts {
        sig.inputs.push(ts);
    } else {
        let ts: TokenStream = quote! {_depot: &mut #salvo::Depot}.into();
        sig.inputs.push(syn::parse_macro_input!(ts as syn::FnArg));
    }
    if let Some(ts) = res_ts {
        sig.inputs.push(ts);
    } else {
        let ts: TokenStream = quote! {_res: &mut #salvo::Response}.into();
        sig.inputs.push(syn::parse_macro_input!(ts as syn::FnArg));
    }
    if let Some(ts) = ctrl_ts {
        sig.inputs.push(ts);
    } else {
        let ts: TokenStream = quote! {_ctrl: &mut #salvo::routing::FlowCtrl}.into();
        sig.inputs.push(syn::parse_macro_input!(ts as syn::FnArg));
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
            (quote! {
                #sdef
                #[async_trait]
                impl #salvo::Handler for #name {
                    #[inline]
                    async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response, ctrl: &mut #salvo::routing::FlowCtrl) {
                        Self::#name(req, depot, res, ctrl).await
                    }
                }
            })
            .into()
        }
        ReturnType::Type(_, _) => (quote! {
            #sdef
            #[async_trait]
            impl #salvo::Handler for #name {
                #[inline]
                async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response, ctrl: &mut #salvo::routing::FlowCtrl) {
                    #salvo::Writer::write(Self::#name(req, depot, res, ctrl).await, req, depot, res).await;
                }
            }
        })
        .into(),
    }
}

fn salvo_crate() -> syn::Ident {
    match crate_name("salvo_core").or_else(|_| crate_name("salvo")) {
        Ok(salvo) => match salvo {
            FoundCrate::Itself => Ident::new("crate", Span::call_site()),
            FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
        },
        Err(_) => Ident::new("crate", Span::call_site()),
    }
}

fn parse_input_type(input: &syn::FnArg) -> InputType {
    if let syn::FnArg::Typed(p) = input {
        if let syn::Type::Reference(ty) = &*p.ty {
            if let syn::Type::Path(nty) = &*ty.elem {
                // the last ident for path type is the real type
                // such as:
                // `::std::vec::Vec` is `Vec`
                // `Vec` is `Vec`
                let ident = &nty.path.segments.last().unwrap().ident;
                if ident == "Request" {
                    InputType::Request
                } else if ident == "Response" {
                    InputType::Response
                } else if ident == "Depot" {
                    InputType::Depot
                } else if ident == "FlowCtrl" {
                    InputType::FlowCtrl
                } else {
                    println!("==============ident: {:?}", ident);
                    InputType::UnKnow
                }
            } else {
                InputType::UnKnow
            }
        } else {
            // like owned type or other type
            InputType::NoReferenceArg
        }
    } else {
        // like self on fn
        InputType::UnKnow
    }
}
