use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::spanned::Spanned;
use syn::{Expr, Ident, ImplItem, Item, Lit, Pat, ReturnType, Signature, Type};

use crate::doc_comment::CommentAttributes;
use crate::{
    Array, DiagLevel, DiagResult, Diagnostic, InputType, Operation, omit_type_path_lifetimes,
    parse_input_type,
};

mod attr;
pub(crate) use attr::EndpointAttr;

/// Build a valid identifier usable as a suffix in generated function names from an
/// arbitrary self type. Any character that is not allowed in an identifier (`:`, `<`,
/// `>`, `,`, whitespace, ...) is replaced with `_`, so qualified or generic types such
/// as `path::Foo` or `Foo<T>` no longer panic the way `Ident::new(ty.to_string())` did.
fn type_name_suffix(ty: &Type) -> Ident {
    let raw = ty.to_token_stream().to_string();
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    // An identifier may not be empty nor start with a digit.
    if sanitized.is_empty() || sanitized.starts_with(|c: char| c.is_ascii_digit()) {
        sanitized.insert(0, '_');
    }
    Ident::new(&sanitized, Span::call_site())
}

fn metadata(
    salvo: &Ident,
    oapi: &Ident,
    attr: &EndpointAttr,
    name: &Ident,
    type_path: &TokenStream,
    mut modifiers: Vec<TokenStream>,
) -> DiagResult<TokenStream> {
    let tfn = Ident::new(
        &format!("__macro_gen_oapi_endpoint_type_id_{name}"),
        Span::call_site(),
    );
    let cfn = Ident::new(
        &format!("__macro_gen_oapi_endpoint_creator_{name}"),
        Span::call_site(),
    );
    let opt = Operation::new(attr);
    modifiers.append(opt.modifiers()?.as_mut());
    let mut status_code_tokens = Vec::with_capacity(attr.status_codes.len());
    for expr in &attr.status_codes {
        match expr {
            Expr::Lit(expr_lit) => match &expr_lit.lit {
                Lit::Int(lit_int) => {
                    // Validate the literal at expansion time so an out-of-range value
                    // produces a clear compile error instead of a runtime `unwrap`
                    // panic during route registration.
                    let code: u16 = lit_int.base10_parse().map_err(|e| {
                        Diagnostic::spanned(lit_int.span(), DiagLevel::Error, e.to_string())
                    })?;
                    if !(100..=599).contains(&code) {
                        return Err(Diagnostic::spanned(
                            lit_int.span(),
                            DiagLevel::Error,
                            format!(
                                "invalid HTTP status code `{code}`: must be in the range 100..=599"
                            ),
                        ));
                    }
                    status_code_tokens.push(quote! {
                        #salvo::http::StatusCode::from_u16(#code)
                            .expect("status code validated at compile time")
                    });
                }
                _ => {
                    return Err(Diagnostic::spanned(
                        expr_lit.span(),
                        DiagLevel::Error,
                        "`status_codes` entries must be integer literals or `StatusCode` expressions",
                    ));
                }
            },
            _ => status_code_tokens.push(quote! { #expr }),
        }
    }
    let status_codes = Array::from_iter(status_code_tokens);
    let modifiers = if modifiers.is_empty() {
        None
    } else {
        Some(quote! {{
            let components = &mut components;
            let operation = &mut operation;
            #(#modifiers)*
        }})
    };
    let stream = quote! {
        fn #tfn() -> ::std::any::TypeId {
            ::std::any::TypeId::of::<#type_path>()
        }
        fn #cfn() -> #oapi::oapi::Endpoint {
            let mut components = #oapi::oapi::Components::new();
            let status_codes: &[#salvo::http::StatusCode] = &#status_codes;
            let mut operation = #oapi::oapi::Operation::new();
            #modifiers
            if operation.operation_id.is_none() {
                operation.operation_id = Some(#oapi::oapi::naming::assign_name::<#type_path>(#oapi::oapi::naming::NameRule::Auto));
            }
            if !status_codes.is_empty() {
                let responses = std::ops::DerefMut::deref_mut(&mut operation.responses);
                responses.retain(|k,_| {
                    if let Ok(code) = <#salvo::http::StatusCode as std::str::FromStr>::from_str(k) {
                        status_codes.contains(&code)
                    } else {
                        true
                    }
                });
            }
            #oapi::oapi::Endpoint{
                operation,
                components,
            }
        }
        #oapi::oapi::__private::inventory::submit! {
            #oapi::oapi::EndpointRegistry::save(#tfn, #cfn)
        }
    };
    Ok(stream)
}
pub(crate) fn generate(mut attr: EndpointAttr, input: Item) -> syn::Result<TokenStream> {
    let salvo = crate::salvo_crate();
    let oapi = crate::oapi_crate();
    match input {
        Item::Fn(mut item_fn) => {
            let attrs = item_fn
                .attrs
                .iter()
                .filter(|attr| !attr.path().is_ident("endpoint"))
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

            attr.doc_comments = Some(CommentAttributes::from_attributes(&docs).0);
            attr.deprecated = if attrs.iter().any(|attr| attr.path().is_ident("deprecated")) {
                Some(true)
            } else {
                None
            };

            let (hfn, modifiers) = handle_fn(&salvo, &oapi, sig)?;
            let meta = metadata(&salvo, &oapi, &attr, name, &quote! { #name }, modifiers)?;
            Ok(quote! {
                #sdef
                #[#salvo::async_trait]
                impl #salvo::Handler for #name {
                    #hfn
                }
                #meta
            })
        }
        Item::Impl(item_impl) => {
            let attrs = &item_impl.attrs;

            // The generated metadata registers the endpoint through top-level, non-generic
            // helper functions (`TypeId::of::<Ty>()` / `assign_name::<Ty>()`) submitted to a
            // static inventory. A generic `impl<T> Foo<T>` has no single concrete type to
            // register, and the impl's parameters are out of scope in those helpers, so
            // reject such impls with a clear error instead of emitting uncompilable code.
            // (A concrete instantiation like `impl Foo<String>` carries no params here and
            // is still accepted.)
            if !item_impl.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(
                    &item_impl.generics,
                    "#[endpoint] does not support generic `impl` blocks; \
                     implement it on a concrete type",
                ));
            }

            attr.doc_comments = Some(CommentAttributes::from_attributes(attrs).0);
            attr.deprecated = if attrs.iter().any(|attr| attr.path().is_ident("deprecated")) {
                Some(true)
            } else {
                None
            };

            let mut hmtd = None;
            for item in &item_impl.items {
                if let ImplItem::Fn(method) = item
                    && method.sig.ident == Ident::new("handle", Span::call_site())
                {
                    hmtd = Some(method);
                }
            }
            let Some(hmtd) = hmtd else {
                return Err(syn::Error::new_spanned(
                    item_impl.impl_token,
                    "missing handle function",
                ));
            };
            let (hfn, modifiers) = handle_fn(&salvo, &oapi, &hmtd.sig)?;
            let ty = &item_impl.self_ty;
            let (impl_generics, _, where_clause) = &item_impl.generics.split_for_impl();
            // The self type is used verbatim for `TypeId::of` / `assign_name`, while a
            // sanitized identifier derived from it names the generated helper fns. This
            // avoids panicking on qualified self types like `path::Foo`, which the old
            // `Ident::new(ty.to_string())` rejected because of the `:` and spaces.
            let name = type_name_suffix(ty);
            let meta = metadata(&salvo, &oapi, &attr, &name, &quote! { #ty }, modifiers)?;

            Ok(quote! {
                #item_impl
                #[#salvo::async_trait]
                impl #impl_generics #salvo::Handler for #ty #where_clause {
                    #hfn
                }
                #meta
            })
        }
        _ => Err(syn::Error::new_spanned(
            input,
            "#[handler] must added to `impl` or `fn`",
        )),
    }
}

fn handle_fn(
    salvo: &Ident,
    oapi: &Ident,
    sig: &Signature,
) -> syn::Result<(TokenStream, Vec<TokenStream>)> {
    let name = &sig.ident;
    let mut extract_ts = Vec::with_capacity(sig.inputs.len());
    let mut call_args: Vec<Ident> = Vec::with_capacity(sig.inputs.len());
    let mut modifiers = Vec::new();
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
                    // If id like `mut pdata`, then idv is `pdata`;
                    let idv = idv
                        .rsplit_once(' ')
                        .map(|(_, v)| v.to_owned())
                        .unwrap_or(idv);
                    let id = Ident::new(&idv, Span::call_site());
                    let idv = idv.trim_start_matches('_');
                    extract_ts.push(quote!{
                        let #id: #ty = match <#ty as #salvo::Extractible>::extract_with_arg(__macro_gen_req, __macro_gen_depot, #idv).await {
                            Ok(data) => {
                                data
                            },
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
                    modifiers.push(quote! {
                         <#ty as #oapi::oapi::EndpointArgRegister>::register(components, operation, #idv);
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

    let hfn = match &sig.output {
        ReturnType::Default => {
            if sig.asyncness.is_none() {
                quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*)
                    }
                }
            } else {
                quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*).await
                    }
                }
            }
        }
        ReturnType::Type(_, ty) => {
            modifiers.push(quote! {
                <#ty as #oapi::oapi::EndpointOutRegister>::register(components, operation);
            });
            if sig.asyncness.is_none() {
                quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*), __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                    }
                }
            } else {
                quote! {
                    async fn handle(&self, __macro_gen_req: &mut #salvo::Request, __macro_gen_depot: &mut #salvo::Depot, __macro_gen_res: &mut #salvo::Response, __macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*).await, __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                    }
                }
            }
        }
    };
    Ok((hfn, modifiers))
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{Ident, Signature, parse_str};

    use super::{handle_fn, type_name_suffix};

    #[test]
    fn test_handle_fn() {
        let salvo = Ident::new("salvo", proc_macro2::Span::call_site());
        let oapi = Ident::new("salvo_oapi", proc_macro2::Span::call_site());
        let sig: Signature = parse_str("fn hello(name: String)").unwrap();
        let (hfn, modifiers) = handle_fn(&salvo, &oapi, &sig).unwrap();
        let expected_hfn = quote! {
            async fn handle(&self, __macro_gen_req: &mut salvo::Request, __macro_gen_depot: &mut salvo::Depot, __macro_gen_res: &mut salvo::Response, __macro_gen_ctrl: &mut salvo::FlowCtrl) {
                let name: String = match <String as salvo::Extractible>::extract_with_arg(__macro_gen_req, __macro_gen_depot, "name").await {
                    Ok(data) => {
                        data
                    },
                    Err(e) => {
                        e.write(__macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                        // If status code is not set or is not error, set it to 400.
                        let status_code = __macro_gen_res.status_code.unwrap_or_default();
                        if !status_code.is_client_error() && !status_code.is_server_error() {
                            __macro_gen_res.status_code(salvo::http::StatusCode::BAD_REQUEST);
                        }
                        return;
                    }
                };
                Self::hello(name)
            }
        };
        assert_eq!(hfn.to_string(), expected_hfn.to_string());
        assert_eq!(modifiers.len(), 1);
        let expected_modifier = quote! {
            <String as salvo_oapi::oapi::EndpointArgRegister>::register(components, operation, "name");
        };
        assert_eq!(modifiers[0].to_string(), expected_modifier.to_string());
    }

    #[test]
    fn type_name_suffix_sanitizes_qualified_and_generic_types() {
        // Each of these used to panic `Ident::new`; now they yield valid idents.
        for src in ["Foo", "path::to::Foo", "Foo<T>", "Foo<'a, T>"] {
            let ty: syn::Type = parse_str(src).unwrap();
            let id = type_name_suffix(&ty).to_string();
            assert!(!id.is_empty());
            assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
            assert!(!id.starts_with(|c: char| c.is_ascii_digit()));
        }
        // A plain type is unchanged.
        assert_eq!(
            type_name_suffix(&parse_str("Foo").unwrap()).to_string(),
            "Foo"
        );
    }

    #[test]
    fn endpoint_on_impl_accepts_qualified_self_type() {
        let attr: super::EndpointAttr = syn::parse2(quote! {}).unwrap();
        let item: syn::Item = parse_str("impl a::b::Foo { fn handle(&self) {} }").unwrap();
        // This used to panic at `Ident::new("a :: b :: Foo")`; now it succeeds.
        let text = super::generate(attr, item)
            .expect("qualified self type should not panic")
            .to_string();
        // The full self type is used verbatim in the type-level positions...
        assert!(text.contains("TypeId :: of :: < a :: b :: Foo >"));
        assert!(text.contains("assign_name :: < a :: b :: Foo >"));
        // ...while the generated helper fns get a sanitized identifier suffix.
        assert!(text.contains("__macro_gen_oapi_endpoint_type_id_a"));
    }

    #[test]
    fn endpoint_on_generic_impl_is_rejected() {
        // A generic impl has no single concrete type to register in the static
        // inventory, so it must be rejected with a clear error rather than expand
        // to helper fns that reference the out-of-scope parameter.
        let attr: super::EndpointAttr = syn::parse2(quote! {}).unwrap();
        let item: syn::Item = parse_str("impl<T> Foo<T> { fn handle(&self) {} }").unwrap();
        let err = super::generate(attr, item).unwrap_err();
        assert!(err.to_string().contains("generic"));
    }

    #[test]
    fn endpoint_on_concrete_generic_impl_is_accepted() {
        // `impl Foo<String>` carries no generic params and must still work.
        let attr: super::EndpointAttr = syn::parse2(quote! {}).unwrap();
        let item: syn::Item = parse_str("impl Foo<String> { fn handle(&self) {} }").unwrap();
        let text = super::generate(attr, item)
            .expect("concrete instantiation should be accepted")
            .to_string();
        assert!(text.contains("TypeId :: of :: < Foo < String > >"));
    }
}
