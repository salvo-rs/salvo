use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{Ident, ImplItem, Item, Pat, ReturnType, Signature, Type};

use crate::shared::*;

pub(crate) fn generate(input: Item) -> syn::Result<TokenStream> {
    let salvo = salvo_crate();
    match input {
        Item::Fn(mut item_fn) => {
            let attrs = item_fn
                .attrs
                .iter()
                .filter(|attr| !attr.path().is_ident("handler"))
                .collect::<Vec<_>>();
            let vis = &item_fn.vis;
            let sig = &mut item_fn.sig;
            let body = &item_fn.block;
            let name = &sig.ident;
            let docs = item_fn
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("doc"))
                .cloned()
                .collect::<Vec<_>>();

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

            let hfn = handle_fn(&salvo, sig)?;
            Ok(quote! {
                #sdef
                #[#salvo::async_trait]
                impl #salvo::Handler for #name {
                    #hfn
                }
            })
        }
        Item::Impl(item_impl) => {
            let mut hmtd = None;
            for item in &item_impl.items {
                if let ImplItem::Fn(method) = item {
                    if method.sig.ident == Ident::new("handle", Span::call_site()) {
                        hmtd = Some(method);
                    }
                }
            }
            let Some(hmtd) = hmtd else {
                return Err(syn::Error::new_spanned(
                    item_impl.impl_token,
                    "missing handle function",
                ));
            };
            let hfn = handle_fn(&salvo, &hmtd.sig)?;
            let ty = &item_impl.self_ty;
            let (impl_generics, _, where_clause) = &item_impl.generics.split_for_impl();

            Ok(quote! {
                #item_impl
                #[#salvo::async_trait]
                impl #impl_generics #salvo::Handler for #ty #where_clause {
                    #hfn
                }
            })
        }
        _ => Err(syn::Error::new_spanned(
            input,
            "#[handler] must added to `impl` or `fn`",
        )),
    }
}

fn handle_fn(salvo: &Ident, sig: &Signature) -> syn::Result<TokenStream> {
    let name = &sig.ident;
    let mut extract_ts = Vec::with_capacity(sig.inputs.len());
    let mut call_args: Vec<Ident> = Vec::with_capacity(sig.inputs.len());
    for input in &sig.inputs {
        match parse_input_type(input) {
            InputType::Request(_pat) => {
                call_args.push(Ident::new("__macro_gen_req", Span::call_site()));
            }
            InputType::Depot(_pat) => {
                call_args.push(Ident::new("__macro_gen_depot", Span::call_site()));
            }
            InputType::Response(_pat) => {
                call_args.push(Ident::new("__macro_gen_res", Span::call_site()));
            }
            InputType::FlowCtrl(_pat) => {
                call_args.push(Ident::new("__macro_gen_ctrl", Span::call_site()));
            }
            InputType::Unknown => {
                return Err(syn::Error::new_spanned(
                    &sig.inputs,
                    "the inputs parameters must be Request, Depot, Response or FlowCtrl",
                ));
            }
            InputType::NoReference(pat) => {
                if let (Pat::Ident(ident), Type::Path(ty)) = (&*pat.pat, &*pat.ty) {
                    call_args.push(ident.ident.clone());
                    let ty = omit_type_path_lifetimes(ty);
                    let idv = pat.pat.to_token_stream().to_string();
                    let idv = idv
                        .rsplit_once(' ')
                        .map(|(_, v)| v.to_owned())
                        .unwrap_or(idv);
                    let id = Ident::new(&idv, Span::call_site());
                    let idv = idv.trim_start_matches('_');

                    extract_ts.push(quote!{
                        let #id: #ty = match <#ty as #salvo::Extractible>::extract_with_arg(__macro_gen_req, #idv).await {
                            Ok(data) => data,
                            Err(e) => {
                                e.write(__macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                                // If status code is not set or is not error, set it to 400.
                                let status_code = __macro_gen_res.status_code.unwrap_or_default();
                                if !status_code.is_client_error() && !status_code.is_server_error() {
                                    __macro_gen_res.status_code(#salvo::http::StatusCode::BAD_REQUEST);
                                }
                                return;
                            }
                        };
                    });
                } else {
                    return Err(syn::Error::new_spanned(pat, "invalid param definition"));
                }
            }
            InputType::Receiver(_) => {
                call_args.push(Ident::new("self", Span::call_site()));
            }
        }
    }

    match sig.output {
        ReturnType::Default => {
            if sig.asyncness.is_none() {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*)
                    }
                })
            } else {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*).await
                    }
                })
            }
        }
        ReturnType::Type(_, _) => {
            if sig.asyncness.is_none() {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*), __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                    }
                })
            } else {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*).await, __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                    }
                })
            }
        }
    }
}
