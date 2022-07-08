use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, Ident, ItemFn, ItemImpl, Meta, NestedMeta, Pat, ReturnType, Token, Type};

use crate::shared::*;
use crate::Item;

pub(crate) fn generate(internal: bool, input: Item) -> syn::Result<TokenStream2> {
    match input {
        Item::Fn(mut item_fn) => {
            let attrs = &item_fn.attrs;
            let vis = &item_fn.vis;
            let sig = &mut item_fn.sig;
            if sig.inputs.len() > 4 {
                return Err(syn::Error::new_spanned(sig.fn_token, "too many args in handle function"));
            }

            let body = &item_fn.block;
            let name = &sig.ident;
            let docs = item_fn
                .attrs
                .iter()
                .filter(|attr| attr.path.is_ident("doc"))
                .cloned()
                .collect::<Vec<_>>();

            let salvo = salvo_crate(internal);

            let mut extract_ts = Vec::with_capacity(sig.inputs.len());
            let mut call_args: Vec<Ident> = Vec::with_capacity(sig.inputs.len());
            for input in &sig.inputs {
                match parse_input_type(input) {
                    InputType::Request(_pat) => {
                        call_args.push(Ident::new("req", Span::call_site()));
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
                        return Err(syn::Error::new_spanned(
                            &sig.inputs,
                            "the inputs parameters must be Request, Depot, Response or FlowCtrl",
                        ))
                    }
                    InputType::NoReference(pat) => {
                        if let (Pat::Ident(ident), Type::Path(ty)) = (&*pat.pat, &*pat.ty) {
                            call_args.push(ident.ident.clone());
                            // Maybe extractible type.
                            let id = &pat.pat;
                            let (ty, lcount) = omit_type_path_lifetimes(ty);
                            if lcount > 1 {
                                return Err(syn::Error::new_spanned(
                                    pat,
                                    "Only one lifetime is allowed for `Extractible` type.",
                                ));
                            }

                            extract_ts.push(quote! {
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
                            return Err(syn::Error::new_spanned(pat, "Invalid param definition."));
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
                        Ok(quote! {
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
                    } else {
                        Ok(quote! {
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
                    }
                }
                ReturnType::Type(_, _) => {
                    if sig.asyncness.is_none() {
                        Ok(quote! {
                            #sdef
                            #[async_trait]
                            impl #salvo::Handler for #name {
                                #[inline]
                                async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot, res: &mut #salvo::Response, ctrl: &mut #salvo::routing::FlowCtrl) {
                                    #salvo::Writer::write(Self::#name(#(#call_args),*), req, depot, res).await;
                                }
                            }
                        })
                    } else {
                        Ok(quote! {
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
                    }
                }
            }
        }
        Item::Impl(mut item_impl) => {
            Ok(quote! {
                #item_impl
            })
        }
    }
}
