use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::ext::IdentExt;
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
            let methods = item_impl
                .items
                .iter()
                .filter_map(|item| match item {
                    ImplItem::Fn(method)
                        if method.sig.ident == Ident::new("handle", Span::call_site()) =>
                    {
                        Some(method)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if methods.is_empty() {
                return Err(syn::Error::new_spanned(
                    item_impl.impl_token,
                    "missing handle function",
                ));
            }
            let ty = &item_impl.self_ty;
            let (impl_generics, _, where_clause) = item_impl.generics.split_for_impl();
            let mut implementations = Vec::new();
            for method in methods {
                let cfg = cfg_attributes(&method.attrs)?;
                match handle_fn(&salvo, &method.sig) {
                    Ok(hfn) => implementations.push(quote! {
                        #(#cfg)*
                        #[#salvo::async_trait]
                        impl #impl_generics #salvo::Handler for #ty #where_clause { #hfn }
                    }),
                    Err(error) if !cfg.is_empty() => {
                        let error = error.into_compile_error();
                        implementations.push(quote! { #(#cfg)* const _: () = { #error }; });
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(quote! { #item_impl #(#implementations)* })
        }
        _ => Err(syn::Error::new_spanned(
            input,
            "#[handler] must be applied to an `impl` block or `fn`",
        )),
    }
}

fn handle_fn(salvo: &Ident, sig: &Signature) -> syn::Result<TokenStream> {
    let name = &sig.ident;
    let mut extract_ts = Vec::with_capacity(sig.inputs.len());
    let mut call_args: Vec<Ident> = Vec::with_capacity(sig.inputs.len());
    let mut needs_conn = false;
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
            InputType::ConnCtrl(_pat) => {
                needs_conn = true;
                call_args.push(Ident::new("__macro_gen_conn", Span::call_site()));
            }
            InputType::Unknown => {
                return Err(syn::Error::new_spanned(
                    &sig.inputs,
                    "the inputs parameters must be Request, Depot, Response, FlowCtrl or ConnCtrl",
                ));
            }
            InputType::NoReference(pat) => {
                if let (Pat::Ident(pat_ident), Type::Path(ty)) = (&*pat.pat, &*pat.ty) {
                    let id = pat_ident.ident.clone();
                    call_args.push(id.clone());
                    let ty = omit_type_path_lifetimes(ty);
                    // Derive the extractor arg name from the parsed identifier
                    // directly. The previous code re-stringified the whole pattern
                    // and rebuilt it with `Ident::new`, which panics on a raw
                    // identifier such as `r#type`. `unraw` drops a leading `r#`.
                    let idv = id.unraw().to_string();
                    let idv = idv.trim_start_matches('_');

                    extract_ts.push(quote!{
                        let #id: #ty = match <#ty as #salvo::Extractible>::extract_with_arg(__macro_gen_req, __macro_gen_depot, #idv).await {
                            Ok(data) => data,
                            Err(e) => {
                                #salvo::Writer::write(e, __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
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
    let conn_init = needs_conn.then(|| {
        quote! {
            let __macro_gen_conn = &mut __macro_gen_ctrl.conn().clone();
        }
    });

    match sig.output {
        ReturnType::Default => {
            if sig.asyncness.is_none() {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*)
                    }
                })
            } else {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*).await
                    }
                })
            }
        }
        ReturnType::Type(..) => {
            if sig.asyncness.is_none() {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*), __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                    }
                })
            } else {
                Ok(quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*).await, __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                    }
                })
            }
        }
    }
}

#[cfg(test)]
mod conditional_diagnostic_tests {
    #[test]
    fn invalid_conditional_handle_emits_a_conditional_diagnostic() {
        let input = syn::parse_quote! {
            impl Service {
                #[cfg(all())]
                fn handle(&self, _invalid: &[u32]) {}
            }
        };
        let generated = super::generate(input).unwrap().to_string();
        assert!(generated.contains("compile_error"));
        assert!(generated.contains("the inputs parameters must be Request"));
        assert!(generated.contains("cfg (all ())"));
    }
}
