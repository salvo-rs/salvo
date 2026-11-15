use std::sync::LazyLock;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use regex::Regex;
use syn::parse::Parser;
use syn::{
    Attribute, FnArg, Generics, Ident, ImplItem, ImplItemFn, Item, Token, Type, parse_quote,
};

use crate::utils::{cfg_attributes, oapi_crate, salvo_crate};

pub(crate) fn generate(input: Item) -> syn::Result<TokenStream> {
    match input {
        Item::Impl(mut item_impl) => {
            let mut helpers = Vec::new();
            for (index, item) in item_impl.items.iter_mut().enumerate() {
                if let ImplItem::Fn(method) = item {
                    let cfg = cfg_attributes(&method.attrs)?;
                    match rewrite_method(
                        item_impl.generics.clone(),
                        item_impl.self_ty.as_ref(),
                        method,
                    ) {
                        Ok(Some(helper)) => helpers.push(ImplItem::Fn(helper)),
                        Ok(None) => {}
                        Err(error) if !cfg.is_empty() => {
                            let name = Ident::new(
                                &format!("__SALVO_CRAFT_ERROR_{index}"),
                                Span::call_site(),
                            );
                            let error = error.into_compile_error();
                            *item = ImplItem::Verbatim(
                                quote! { #(#cfg)* const #name: () = { #error }; },
                            );
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            item_impl.items.extend(helpers);
            Ok(item_impl.into_token_stream())
        }
        Item::Fn(_) => Ok(input.into_token_stream()),
        _ => Err(syn::Error::new_spanned(
            input,
            "#[craft] must be applied to an `impl` block",
        )),
    }
}

const REGEX_STR: &str = r#"(?s)#\s*\[\s*(::)?\s*([A-Za-z_][A-Za-z0-9_]*\s*::\s*)*\s*craft\s*\(\s*(?P<name>handler|endpoint)\s*(?P<content>\(.*\))?\s*\)\s*\]"#;

/// Compiled once per process instead of per `#[craft]` method.
static CRAFT_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(REGEX_STR).expect("regex compile should not fail"));

fn take_method_macro(item_fn: &mut ImplItemFn) -> syn::Result<Option<Attribute>> {
    let mut index: Option<usize> = None;
    let mut new_attr: Option<Attribute> = None;
    let re = &*CRAFT_ATTR_RE;
    for (idx, attr) in &mut item_fn.attrs.iter().enumerate() {
        if !(match attr.path().segments.last() {
            Some(segment) => segment.ident == "craft",
            None => false,
        }) {
            continue;
        }
        let attr_str = attr.to_token_stream().to_string().trim().to_owned();

        if let Some(caps) = re.captures(&attr_str)
            && let Some(name) = caps.name("name")
        {
            let name = name.as_str();
            let content = caps
                .name("content")
                .map(|c| c.as_str().to_owned())
                .unwrap_or_default();
            let ts: TokenStream = match name {
                "handler" => {
                    format!("#[{}::{name}{content}]", salvo_crate().to_token_stream()).parse()?
                }
                "endpoint" => format!(
                    "#[{}::oapi::{name}{content}]",
                    oapi_crate().to_token_stream()
                )
                .parse()?,
                _ => {
                    unreachable!()
                }
            };
            new_attr = Attribute::parse_outer.parse2(ts)?.into_iter().next();
            index = Some(idx);
            continue;
        }

        return Err(syn::Error::new_spanned(
            item_fn,
            format!(
                "The attribute macro `{attr_str}` on a method must be filled with sub-attributes, such as '#[craft(handler)]', '#[craft(endpoint)]', or '#[craft(endpoint(...))]'."
            ),
        ));
    }
    if let Some(index) = index {
        item_fn.attrs.remove(index);
        return Ok(new_attr);
    }
    Ok(None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
enum FnReceiver {
    None,
    Ref,
    Arc,
}

impl FnReceiver {
    fn from_method(method: &ImplItemFn) -> syn::Result<Self> {
        let Some(recv) = method.sig.receiver() else {
            return Ok(Self::None);
        };
        match &recv.kind {
            syn::ReceiverKind::Reference(_, _, None) if recv.mutability.is_none() => Ok(Self::Ref),
            syn::ReceiverKind::Typed(_, ty) => {
                let is_self = |ty: &Type| matches!(ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident("Self"));
                let is_arc = |ty: &Type| {
                    matches!(ty, Type::Path(path) if path.qself.is_none() && path.path.segments.last().is_some_and(|segment| {
                        segment.ident == "Arc"
                            && matches!(&segment.arguments, syn::PathArguments::AngleBracketed(args)
                                if args.args.len() == 1 && matches!(args.args.first(), Some(syn::GenericArgument::Type(ty)) if is_self(ty)))
                    }))
                };
                if matches!(ty.as_ref(), Type::Reference(reference) if reference.mutability.is_none() && is_self(&reference.elem))
                {
                    Ok(Self::Ref)
                } else if is_arc(ty)
                    || matches!(ty.as_ref(), Type::Reference(reference) if reference.mutability.is_none() && is_arc(&reference.elem))
                {
                    Ok(Self::Arc)
                } else {
                    Err(syn::Error::new_spanned(
                        method,
                        "#[craft] method receiver must be '&self', 'Arc<Self>' or '&Arc<Self>'",
                    ))
                }
            }
            _ => Err(syn::Error::new_spanned(
                method,
                "#[craft] method receiver must be '&self', 'Arc<Self>' or '&Arc<Self>'",
            )),
        }
    }
}

fn rewrite_method(
    impl_generics: Generics,
    self_ty: &Type,
    method: &mut ImplItemFn,
) -> syn::Result<Option<ImplItemFn>> {
    let Some(macro_attr) = take_method_macro(method)? else {
        return Ok(None);
    };
    method.sig.asyncness = Some(Token![async](Span::call_site()));
    let salvo = salvo_crate();
    let handler = quote!(#salvo::Handler);
    let method_name = method.sig.ident.clone();
    let vis = method.vis.clone();
    let factory_where = method.sig.generics.where_clause.clone();
    let mut attrs = method.attrs.clone();
    let style = FnReceiver::from_method(method)?;
    let stateless_generic = style == FnReceiver::None && !impl_generics.params.is_empty();
    let borrowed_arc = method.sig.receiver().is_some_and(|recv| {
        matches!(&recv.kind, syn::ReceiverKind::Typed(_, ty) if matches!(ty.as_ref(), Type::Reference(_)))
    });
    let helper_name = Ident::new(
        &format!(
            "__salvo_craft_{}",
            method_name.to_string().trim_start_matches("r#")
        ),
        method_name.span(),
    );
    let mut helper = method.clone();
    helper.sig.ident = helper_name.clone();
    helper.vis = syn::Visibility::Inherited;
    helper
        .attrs
        .retain(|attr| !attr.path().is_ident("deprecated"));
    if stateless_generic {
        let dispatch = Ident::new(
            &format!(
                "__salvo_dispatch_{}",
                helper_name.to_string().trim_start_matches('_')
            ),
            method_name.span(),
        );
        let creator = Ident::new(
            &format!(
                "__salvo_metadata_{}",
                helper_name.to_string().trim_start_matches('_')
            ),
            method_name.span(),
        );
        let mut lifetime_name = "__salvo_craft_request".to_owned();
        while impl_generics
            .lifetimes()
            .chain(method.sig.generics.lifetimes())
            .any(|parameter| parameter.lifetime.ident == lifetime_name)
        {
            lifetime_name.push('_');
        }
        let lifetime = syn::Lifetime::new(&format!("'{lifetime_name}"), Span::mixed_site());
        let is_endpoint = macro_attr
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "endpoint");
        let oapi = oapi_crate();
        let endpoint_args = match &macro_attr.meta {
            syn::Meta::List(list) => list.tokens.clone(),
            _ => TokenStream::new(),
        };
        // Expand extraction and metadata helpers as associated functions, where
        // the original service's type, const and lifetime parameters remain in scope.
        helper.attrs = attrs.clone();
        helper.attrs.push(if is_endpoint {
            parse_quote!(#[#oapi::oapi::__private::__craft_endpoint(#dispatch, #creator, #lifetime, #method_name, (#impl_generics); #endpoint_args)])
        } else {
            parse_quote!(#[#salvo::macros::__craft_handler(#dispatch, #lifetime, (#impl_generics))])
        });
        let metadata_field = is_endpoint.then(|| quote!(creator: fn() -> #oapi::oapi::Endpoint,));
        let metadata_init = is_endpoint.then(|| {
            quote! {
                creator: || Self::#creator(::std::any::TypeId::of::<handle>()),
            }
        });
        let metadata_hook = is_endpoint.then(|| {
            quote! {
                fn type_data(&self) -> Option<&(dyn ::std::any::Any + Send + Sync)> {
                    Some(&self.creator)
                }
            }
        });
        let mut factory: ImplItemFn = parse_quote! {
            #vis fn #method_name() -> impl #handler #factory_where {
                // Function pointers erase the service parameters: even borrowed
                // or non-Send service types can produce an independent handler.
                type Dispatch = for<#lifetime> fn(
                    &#lifetime mut #salvo::Request,
                    &#lifetime mut #salvo::Depot,
                    &#lifetime mut #salvo::Response,
                    &#lifetime mut #salvo::FlowCtrl,
                ) -> ::std::pin::Pin<::std::boxed::Box<
                    dyn ::std::future::Future<Output = ()> + ::std::marker::Send + #lifetime
                >>;
                #[derive(Debug)]
                struct handle { dispatch: Dispatch, #metadata_field }
                #[#salvo::async_trait]
                impl #handler for handle {
                    #metadata_hook
                    async fn handle(&self, req: &mut #salvo::Request, depot: &mut #salvo::Depot,
                        res: &mut #salvo::Response, ctrl: &mut #salvo::FlowCtrl) {
                        (self.dispatch)(req, depot, res, ctrl).await;
                    }
                }
                handle { dispatch: Self::#dispatch, #metadata_init }
            }
        };
        factory.attrs.append(&mut attrs);
        *method = factory;
        return Ok(Some(helper));
    }
    let mut arguments = Vec::new();
    for (index, input) in method.sig.inputs.iter_mut().enumerate() {
        if let FnArg::Typed(input) = input {
            let ident = match input.pat.as_ref() {
                syn::Pat::Ident(pat) if pat.subpat.is_none() => pat.ident.clone(),
                _ => Ident::new(&format!("__salvo_craft_arg_{index}"), Span::call_site()),
            };
            input.pat = Box::new(parse_quote!(#ident));
            arguments.push(ident);
        }
    }
    // The body stays in the service impl. Only the forwarding signature belongs to
    // the generated handler, where `Self` must still denote the service type.
    struct ServiceSelf<'a>(&'a Type);
    impl syn::visit_mut::VisitMut for ServiceSelf<'_> {
        // Nested items introduce their own scope and cannot capture the service's
        // Self or impl parameters. Keep their types and expressions untouched.
        fn visit_item_mut(&mut self, _item: &mut syn::Item) {}

        fn visit_type_mut(&mut self, ty: &mut Type) {
            syn::visit_mut::visit_type_mut(self, ty);
            if let Type::Path(path) = ty
                && path.qself.is_none()
                && path
                    .path
                    .segments
                    .first()
                    .is_some_and(|segment| segment.ident == "Self")
            {
                let service = self.0;
                let rest: syn::punctuated::Punctuated<syn::PathSegment, Token![::]> =
                    path.path.segments.iter().skip(1).cloned().collect();
                *ty = if rest.is_empty() {
                    parse_quote!(#service)
                } else {
                    parse_quote!(<#service>::#rest)
                };
            }
        }

        fn visit_expr_path_mut(&mut self, expression: &mut syn::ExprPath) {
            syn::visit_mut::visit_expr_path_mut(self, expression);
            if expression.qself.is_none()
                && expression.path.segments.len() > 1
                && expression
                    .path
                    .segments
                    .first()
                    .is_some_and(|segment| segment.ident == "Self")
            {
                let service = self.0;
                let suffix: syn::punctuated::Punctuated<syn::PathSegment, Token![::]> =
                    expression.path.segments.iter().skip(1).cloned().collect();
                let mut replacement: syn::ExprPath = parse_quote!(<#service>::#suffix);
                replacement.attrs = ::std::mem::take(&mut expression.attrs);
                *expression = replacement;
            }
        }
    }
    use syn::visit_mut::VisitMut;
    ServiceSelf(self_ty).visit_signature_mut(&mut method.sig);
    let call = match style {
        FnReceiver::None => quote!(<#self_ty>::#helper_name(#(#arguments),*).await),
        FnReceiver::Ref => {
            quote!(self.__salvo_service.as_ref().#helper_name(#(#arguments),*).await)
        }
        FnReceiver::Arc if borrowed_arc => {
            quote!(self.__salvo_service.#helper_name(#(#arguments),*).await)
        }
        FnReceiver::Arc => quote!(self.__salvo_service.clone().#helper_name(#(#arguments),*).await),
    };
    method.block = parse_quote!({ #call });
    let mut new_method: ImplItemFn = match style {
        FnReceiver::None => {
            method.attrs.push(macro_attr);
            parse_quote! {
                #vis fn #method_name() -> impl #handler #factory_where {

                    #method

                    #method_name
                }
            }
        }
        style => {
            let (receiver, output) = match style {
                FnReceiver::Ref => (quote!(&self), quote!(::std::sync::Arc::new(self.clone()))),
                FnReceiver::Arc => (quote!(self: &::std::sync::Arc<Self>), quote!(self.clone())),
                _ => unreachable!(),
            };
            // `#[endpoint]` on an `impl` block reads doc/deprecated attributes from the
            // impl block itself (see oapi-macros `endpoint.rs`). Forward those attributes
            // from the original method onto the generated impl block so the OpenAPI
            // description and `deprecated` flag are picked up. Restrict to the
            // `#[doc = "..."]` NameValue form so list-form attributes like
            // `#[doc(alias = "...")]`, which are valid on methods but not on impl
            // blocks, do not break compilation.
            let forwarded_attrs: Vec<Attribute> = method
                .attrs
                .iter()
                .filter(|a| {
                    if a.path().is_ident("doc") {
                        matches!(a.meta, syn::Meta::NameValue(_))
                    } else {
                        a.path().is_ident("deprecated")
                    }
                })
                .cloned()
                .collect();
            method.sig.inputs[0] = FnArg::Receiver(parse_quote!(&self));
            method.sig.ident = Ident::new("handle", Span::call_site());
            // Use `split_for_impl` so the generated `handle` struct is referred to
            // with its *type* generics (`handle<T>`) in impl headers, while the
            // `impl` keeps the bounded `impl<T: Bound>` form and the where-clause.
            // The previous code derived the type arguments from `self_ty`, which is
            // wrong whenever they differ from the impl's parameters (e.g.
            // `impl<T> Foo<Wrapper<T>>` would emit `handle<Wrapper<T>>`).
            // In the local wrapper, `Self` names the wrapper rather than the service.
            // Keep predicates from the service impl attached to the service type.
            let mut handler_generics = impl_generics.clone();
            if let Some(predicates) = &method.sig.generics.where_clause {
                handler_generics
                    .make_where_clause()
                    .predicates
                    .extend(predicates.predicates.clone());
            }
            ServiceSelf(self_ty).visit_generics_mut(&mut handler_generics);
            let (impl_g, ty_g, where_c) = handler_generics.split_for_impl();
            parse_quote! {
                #vis fn #method_name(#receiver) -> impl #handler #factory_where {
                    #[allow(non_camel_case_types)]
                    // Named fields avoid introducing a tuple constructor into
                    // the value namespace of the user's parameter bindings.
                    struct handle #impl_g #where_c {
                        __salvo_service: ::std::sync::Arc<#self_ty>,
                    }
                    #(#forwarded_attrs)*
                    #macro_attr
                    impl #impl_g handle #ty_g #where_c {
                        #method
                    }
                    handle { __salvo_service: #output }
                }
            }
        }
    };
    new_method.attrs.append(&mut attrs);
    *method = new_method;
    // println!("{}", method.to_token_stream());
    Ok(Some(helper))
}

#[cfg(test)]
mod tests {
    use regex::Regex;
    use syn::parse_quote;

    use super::{FnReceiver, REGEX_STR};

    #[test]
    fn extract_attribute() {
        let re = Regex::new(REGEX_STR).unwrap();

        let texts = vec![
            r###"#[:: craft(endpoint(responses((status_code = 400, description = "[(Wrong)] request parameters."))))]"###,
            r###"#[ xx ::craft(handler())]"###,
            r###"#[::xx::craft(endpoint(simple_text))] "###,
            r###"#[craft(handler)]"###,
        ];
        for text in texts {
            for caps in re.captures_iter(text) {
                println!(
                    "name={}, content={:?}",
                    caps.name("name").unwrap().as_str(),
                    caps.name("content").map(|c| c.as_str().to_owned())
                )
            }
        }
    }

    #[test]
    fn test_fn_receiver() {
        let method: syn::ImplItemFn = parse_quote! {
            fn my_method(&self) {}
        };
        assert_eq!(FnReceiver::from_method(&method).unwrap(), FnReceiver::Ref);

        let method: syn::ImplItemFn = parse_quote! {
            fn my_method(self: &Self) {}
        };
        assert_eq!(FnReceiver::from_method(&method).unwrap(), FnReceiver::Ref);

        let method: syn::ImplItemFn = parse_quote! {
            fn my_method(self: ::std::sync::Arc<Self>) {}
        };
        assert_eq!(FnReceiver::from_method(&method).unwrap(), FnReceiver::Arc);

        let method: syn::ImplItemFn = parse_quote! {
            fn my_method(self: &::std::sync::Arc<Self>) {}
        };
        assert_eq!(FnReceiver::from_method(&method).unwrap(), FnReceiver::Arc);

        let method: syn::ImplItemFn = parse_quote! {
            fn my_method() {}
        };
        assert_eq!(FnReceiver::from_method(&method).unwrap(), FnReceiver::None);

        let method: syn::ImplItemFn = parse_quote! {
            fn my_method(self: Box<Self>) {}
        };
        assert!(FnReceiver::from_method(&method).is_err());
    }
}

#[cfg(test)]
mod conditional_diagnostic_tests {
    #[test]
    fn enabled_conditional_receiver_errors_keep_the_original_message() {
        let input = syn::parse_quote! {
            impl Service {
                #[cfg(all())]
                #[craft(handler)]
                fn invalid(&mut self) {}
            }
        };
        let generated = super::generate(input).unwrap().to_string();
        assert!(generated.contains("compile_error"));
        assert!(generated.contains("method receiver must be"));
        assert!(generated.contains("cfg (all ())"));
    }
}
