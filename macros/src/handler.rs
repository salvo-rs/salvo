use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, Type, Pat, AttributeArgs, Ident, ItemFn, Meta, NestedMeta, ReturnType};

use crate::shared::*;

pub(crate) fn fn_handler(args: TokenStream, mut item_fn: ItemFn) -> TokenStream {
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
                    let (ty, lcount) = omit_type_path_lifetimes(ty);
                    if lcount > 1 {
                        return syn::Error::new_spanned(pat, "Only one lifetime is allowed for `Extractible` type.")
                        .to_compile_error().into();
                    }

                    extract_ts.push(quote!{
                        let #id: #ty = match req.extract().await {
                            Ok(data) => data,
                            Err(e) => {
                                #salvo::__private::tracing::error!(error = ?e, "failed to extract data");
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
