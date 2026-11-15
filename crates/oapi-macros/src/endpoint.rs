use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Expr, Ident, ImplItem, Item, Lit, Pat, ReturnType, Signature, Type};

use crate::doc_comment::CommentAttributes;
use crate::shared::{cfg_attributes, fresh_binding, reserved_identifiers};
use crate::{
    Array, DiagLevel, DiagResult, Diagnostic, InputType, Operation, omit_type_path_lifetimes,
    parse_input_type, unwrapped_type,
};

mod attr;
pub(crate) use attr::EndpointAttr;

/// Encode the concrete type syntax injectively into an identifier suffix.
/// Hex-encoding every UTF-8 byte keeps punctuation, underscores and Unicode
/// distinct, rather than allowing different owners to share sanitized helpers.
fn type_name_suffix(ty: &Type) -> Ident {
    let raw = ty.to_token_stream().to_string();
    let mut encoded = String::from("type_");
    for byte in raw.bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    Ident::new(&encoded, Span::call_site())
}

fn metadata(
    salvo: &syn::Path,
    oapi: &syn::Path,
    attr: &EndpointAttr,
    name: &Ident,
    type_path: &TokenStream,
    modifiers: Vec<TokenStream>,
    cfg: &[syn::Attribute],
) -> DiagResult<TokenStream> {
    metadata_inner(salvo, oapi, attr, name, type_path, modifiers, cfg, None)
}

fn metadata_inner(
    salvo: &syn::Path,
    oapi: &syn::Path,
    attr: &EndpointAttr,
    name: &Ident,
    type_path: &TokenStream,
    mut modifiers: Vec<TokenStream>,
    cfg: &[syn::Attribute],
    associated: Option<(&Ident, &Ident, &Ident)>,
) -> DiagResult<TokenStream> {
    let name = name.unraw();
    let tfn = Ident::new(
        &format!("__macro_gen_oapi_endpoint_type_id_{name}"),
        Span::call_site(),
    );
    let cfn = Ident::new(
        &format!("__macro_gen_oapi_endpoint_creator_{name}"),
        Span::call_site(),
    );
    let owner: Type = syn::parse2(type_path.clone())?;
    let metadata_trait = Ident::new(
        &format!("__SalvoEndpointMetadata_{}", type_name_suffix(&owner)),
        Span::mixed_site(),
    );
    let metadata_method = Ident::new(
        &format!("__salvo_endpoint_create_{}", type_name_suffix(&owner)),
        Span::mixed_site(),
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
    let modifiers = (!modifiers.is_empty()).then(|| {
        quote! {{
            let components = &mut components;
            let operation = &mut operation;
            #(#modifiers)*
        }}
    });
    let allow_deprecated = attr
        .deprecated
        .unwrap_or_default()
        .then(|| quote!(#[allow(deprecated)]));
    let auto_operation_id = match associated {
        Some((_, method, endpoint_type_id)) => {
            quote!(#oapi::oapi::naming::namer().assign_name(
                #endpoint_type_id,
                ::std::any::type_name_of_val(&Self::#method),
                #oapi::oapi::naming::NameRule::Auto,
            ))
        }
        None => {
            quote!(#oapi::oapi::naming::assign_name::<#type_path>(#oapi::oapi::naming::NameRule::Auto))
        }
    };
    let creator_body = quote! {
        let mut components = #oapi::oapi::Components::new();
        let status_codes: &[#salvo::http::StatusCode] = &#status_codes;
        let mut operation = #oapi::oapi::Operation::new();
        #modifiers
        if operation.operation_id.is_none() {
            operation.assign_auto_operation_id(#auto_operation_id);
        }
        if !status_codes.is_empty() {
            let responses = ::std::ops::DerefMut::deref_mut(&mut operation.responses);
            responses.retain(|k,_| {
                if let Ok(code) = <#salvo::http::StatusCode as ::std::str::FromStr>::from_str(k) {
                    status_codes.contains(&code)
                } else {
                    true
                }
            });
        }
        // Keep manual/untyped responses valid without inferring a body shape.
        if operation.responses.is_empty() {
            operation.responses.insert("default", #oapi::oapi::Response::new("Response"));
        }
        #oapi::oapi::Endpoint { operation, components, }
    };
    if let Some((creator, _, endpoint_type_id)) = associated {
        return Ok(quote! {
            #allow_deprecated
            fn #creator(#endpoint_type_id: ::std::any::TypeId) -> #oapi::oapi::Endpoint { #creator_body }
        });
    }
    let stream = quote! {
        #(#cfg)*
        #allow_deprecated
        const _: () = {
            // Evaluate expressions in the owner's impl scope, where Rust can
            // expand macros without rewriting their tokens or nested Self scopes.
            trait #metadata_trait {
                fn #metadata_method() -> #oapi::oapi::Endpoint;
            }
            impl #metadata_trait for #type_path {
                fn #metadata_method() -> #oapi::oapi::Endpoint { #creator_body }
            }
            fn #tfn() -> ::std::any::TypeId {
                ::std::any::TypeId::of::<#type_path>()
            }
            fn #cfn() -> #oapi::oapi::Endpoint {
                <#type_path as #metadata_trait>::#metadata_method()
            }
            #oapi::oapi::__private::inventory::submit! {
                #oapi::oapi::EndpointRegistry::save(#tfn, #cfn)
            }
        };
    };
    Ok(stream)
}
pub(crate) fn generate(mut attr: EndpointAttr, input: Item) -> syn::Result<TokenStream> {
    let salvo = crate::salvo_crate();
    let oapi = crate::oapi_crate();
    let reserved = reserved_identifiers(input.to_token_stream());
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

            // Only attributes applicable to the generated public type belong
            // here; function-only attributes stay on the associated method.
            let public_attrs = item_fn.attrs.iter().filter(|attr| {
                ["doc", "deprecated", "allow", "warn", "deny", "forbid"]
                    .iter()
                    .any(|name| attr.path().is_ident(name))
            });
            let allow_deprecated = attrs
                .iter()
                .any(|attr| attr.path().is_ident("deprecated"))
                .then(|| quote!(#[allow(deprecated)]));
            let sdef = quote! {
                #(#public_attrs)*
                #[allow(non_camel_case_types)]
                #[derive(Debug)]
                #vis struct #name;
            };
            let inherent = quote! {
                #allow_deprecated
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

            let (hfn, modifiers) = handle_fn(&salvo, &oapi, sig, &reserved)?;
            let meta = metadata(
                &salvo,
                &oapi,
                &attr,
                name,
                &quote! { #name },
                modifiers,
                &[],
            )?;
            let implementations = binding_scope(
                sig,
                body,
                quote! {
                    #inherent
                    #allow_deprecated
                    #[#salvo::async_trait]
                    impl #salvo::Handler for #name { #hfn }
                },
            );
            Ok(quote! { #sdef #implementations #meta })
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
            let name = type_name_suffix(ty);
            let mut implementations = Vec::new();
            for method in methods {
                let cfg = cfg_attributes(&method.attrs)?;
                let (hfn, modifiers) = match handle_fn(&salvo, &oapi, &method.sig, &reserved) {
                    Ok(result) => result,
                    Err(error) if !cfg.is_empty() => {
                        let error = error.into_compile_error();
                        implementations.push(quote! { #(#cfg)* const _: () = { #error }; });
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let meta = metadata(
                    &salvo,
                    &oapi,
                    &attr,
                    &name,
                    &quote! { #ty },
                    modifiers,
                    &cfg,
                )?;
                implementations.push(quote! {
                    #(#cfg)*
                    #[#salvo::async_trait]
                    impl #impl_generics #salvo::Handler for #ty #where_clause { #hfn }
                    #meta
                });
            }
            Ok(quote! { #item_impl #(#implementations)* })
        }
        _ => Err(syn::Error::new_spanned(
            input,
            "#[endpoint] must be added to `impl` or `fn`",
        )),
    }
}

fn binding_scope(sig: &Signature, body: &syn::Block, implementations: TokenStream) -> TokenStream {
    use syn::visit::Visit;

    struct Bindings<'a> {
        name: &'a Ident,
        found: bool,
    }
    impl Bindings<'_> {
        fn matches(&self, ident: &Ident) -> bool {
            ident.unraw() == self.name.unraw()
        }

        fn imported_name(&self, tree: &syn::UseTree, parent: Option<&Ident>) -> bool {
            match tree {
                syn::UseTree::Name(item) if item.ident == "self" => {
                    parent.is_some_and(|ident| self.matches(ident))
                }
                syn::UseTree::Name(item) => self.matches(&item.ident),
                syn::UseTree::Rename(item) => self.matches(&item.rename),
                syn::UseTree::Path(item) => self.imported_name(&item.tree, Some(&item.ident)),
                syn::UseTree::Group(item) => item
                    .items
                    .iter()
                    .any(|tree| self.imported_name(tree, parent)),
                syn::UseTree::Glob(_) => true,
            }
        }

        fn declares_local_name(&self, item: &Item) -> bool {
            match item {
                Item::Fn(item) => self.matches(&item.sig.ident),
                Item::Const(item) => self.matches(&item.ident),
                Item::Static(item) => self.matches(&item.ident),
                Item::Struct(item) if !matches!(item.fields, syn::Fields::Named(_)) => {
                    self.matches(&item.ident)
                }
                Item::Use(item) => self.imported_name(&item.tree, None),
                Item::ForeignMod(item) => item.items.iter().any(|item| match item {
                    syn::ForeignItem::Fn(item) => self.matches(&item.sig.ident),
                    syn::ForeignItem::Static(item) => self.matches(&item.ident),
                    _ => false,
                }),
                _ => false,
            }
        }
    }
    impl<'ast> Visit<'ast> for Bindings<'_> {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            self.found |= self.matches(&pattern.ident);
            syn::visit::visit_pat_ident(self, pattern);
        }

        fn visit_block(&mut self, block: &'ast syn::Block) {
            // Preserve existing resolution beneath local values or imports.
            // An import's namespace cannot be resolved from its syntax alone.
            if block
                .stmts
                .iter()
                .any(|stmt| matches!(stmt, syn::Stmt::Item(item) if self.declares_local_name(item)))
            {
                return;
            }
            syn::visit::visit_block(self, block);
        }

        fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {
            // A nested module has its own item namespace.
        }

        fn visit_item_foreign_mod(&mut self, _: &'ast syn::ItemForeignMod) {
            // Foreign declaration argument names do not bind values.
        }

        fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
            if item.default.is_some() {
                syn::visit::visit_trait_item_fn(self, item);
            }
        }

        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            if item
                .sig
                .generics
                .const_params()
                .any(|param| self.matches(&param.ident))
            {
                return;
            }
            syn::visit::visit_item_fn(self, item);
        }
    }

    let name = &sig.ident;
    let mut bindings = Bindings { name, found: false };
    bindings.visit_signature(sig);
    bindings.visit_block(body);
    if !bindings.found {
        return implementations;
    }
    // Function names can be shadowed by bindings; unit constructors cannot.
    // Leave the user's body/tokens intact and retain the public handler value,
    // Self, relative paths and the endpoint's external metadata registration.
    quote! { const _: () = {
        #[allow(dead_code, non_snake_case)]
        fn #name() {}
        #implementations
    }; }
}

fn handle_fn(
    salvo: &syn::Path,
    oapi: &syn::Path,
    sig: &Signature,
    reserved: &::std::collections::HashSet<String>,
) -> syn::Result<(TokenStream, Vec<TokenStream>)> {
    let mut reserved = reserved.clone();
    let name = &sig.ident;
    let __macro_gen_conn = fresh_binding("__macro_gen_conn", &mut reserved);
    let __macro_gen_ctrl = fresh_binding("__macro_gen_ctrl", &mut reserved);
    let __macro_gen_data = fresh_binding("__macro_gen_data", &mut reserved);
    let __macro_gen_depot = fresh_binding("__macro_gen_depot", &mut reserved);
    let __macro_gen_err = fresh_binding("__macro_gen_err", &mut reserved);
    let __macro_gen_req = fresh_binding("__macro_gen_req", &mut reserved);
    let __macro_gen_res = fresh_binding("__macro_gen_res", &mut reserved);
    let __macro_gen_status_code = fresh_binding("__macro_gen_status_code", &mut reserved);

    let mut extract_ts = Vec::with_capacity(sig.inputs.len());
    let mut call_args: Vec<Ident> = Vec::with_capacity(sig.inputs.len());
    let mut modifiers = Vec::new();
    let mut needs_conn = false;
    for input in &sig.inputs {
        match parse_input_type(input) {
            InputType::Request(_pat) => {
                call_args.push(__macro_gen_req.clone());
            }
            InputType::Depot(_pat) => {
                call_args.push(__macro_gen_depot.clone());
            }
            InputType::Response(_pat) => {
                call_args.push(__macro_gen_res.clone());
            }
            InputType::FlowCtrl(_pat) => {
                call_args.push(__macro_gen_ctrl.clone());
            }
            InputType::ConnCtrl(_pat) => {
                needs_conn = true;
                call_args.push(__macro_gen_conn.clone());
            }
            InputType::Unknown => {
                return Err(syn::Error::new_spanned(
                    &sig.inputs,
                    "the inputs parameters must be Request, Depot, Response, FlowCtrl or ConnCtrl",
                ));
            }
            InputType::NoReference(pat) => {
                if let (Pat::Ident(ident), Type::Path(ty)) = (&*pat.pat, unwrapped_type(&pat.ty)) {
                    let id = ident.ident.clone();
                    call_args.push(id.clone());
                    let ty = omit_type_path_lifetimes(ty);
                    let idv = ident.ident.unraw().to_string();
                    let idv = idv.trim_start_matches('_');
                    extract_ts.push(quote!{
                        let #id: #ty = match <#ty as #salvo::Extractible>::extract_with_arg(#__macro_gen_req, #__macro_gen_depot, #idv).await {
                            Ok(#__macro_gen_data) => {
                                #__macro_gen_data
                            },
                            Err(#__macro_gen_err) => {
                                #salvo::Writer::write(#__macro_gen_err, #__macro_gen_req, #__macro_gen_depot, #__macro_gen_res).await;
                                // If status code is not set or is not error, set it to 400.
                                let #__macro_gen_status_code = #__macro_gen_res.status_code.unwrap_or_default();
                                if !#__macro_gen_status_code.is_client_error() && !#__macro_gen_status_code.is_server_error() {
                                    #__macro_gen_res.status_code(#salvo::http::StatusCode::BAD_REQUEST);
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

    let conn_init = needs_conn.then(|| {
        quote! {
            let #__macro_gen_conn = &mut #__macro_gen_ctrl.conn().clone();
        }
    });
    let hfn = match &sig.output {
        ReturnType::Default => {
            if sig.asyncness.is_none() {
                quote! {
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*)
                    }
                }
            } else {
                quote! {
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*).await
                    }
                }
            }
        }
        ReturnType::Type(_, ty) => {
            fn is_unit(ty: &Type) -> bool {
                match ty {
                    Type::Group(group) => is_unit(&group.elem),
                    Type::Paren(paren) => is_unit(&paren.elem),
                    Type::Tuple(tuple) => tuple.elems.is_empty(),
                    _ => false,
                }
            }
            if !is_unit(ty) {
                modifiers.push(quote! {
                    <#ty as #oapi::oapi::EndpointOutRegister>::register(components, operation);
                });
            }
            if sig.asyncness.is_none() {
                quote! {
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*), #__macro_gen_req, #__macro_gen_depot, #__macro_gen_res).await;
                    }
                }
            } else {
                quote! {
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*).await, #__macro_gen_req, #__macro_gen_depot, #__macro_gen_res).await;
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
    use syn::{Signature, parse_str};

    use super::{handle_fn, type_name_suffix};

    #[test]
    fn test_handle_fn() {
        let salvo: syn::Path = syn::parse_quote!(salvo);
        let oapi: syn::Path = syn::parse_quote!(salvo_oapi);
        let sig: Signature = parse_str("fn hello(name: String)").unwrap();
        let (hfn, modifiers) = handle_fn(&salvo, &oapi, &sig, &Default::default()).unwrap();
        let expected_hfn = quote! {
            async fn handle(&self, __macro_gen_req: &mut salvo::Request, __macro_gen_depot: &mut salvo::Depot, __macro_gen_res: &mut salvo::Response, __macro_gen_ctrl: &mut salvo::FlowCtrl) {
                let name: String = match <String as salvo::Extractible>::extract_with_arg(__macro_gen_req, __macro_gen_depot, "name").await {
                    Ok(__macro_gen_data) => {
                        __macro_gen_data
                    },
                    Err(__macro_gen_err) => {
                        salvo::Writer::write(__macro_gen_err, __macro_gen_req, __macro_gen_depot, __macro_gen_res).await;
                        // If status code is not set or is not error, set it to 400.
                        let __macro_gen_status_code = __macro_gen_res.status_code.unwrap_or_default();
                        if !__macro_gen_status_code.is_client_error() && !__macro_gen_status_code.is_server_error() {
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
        // Every byte is encoded, including plain identifiers and underscores.
        assert_eq!(
            type_name_suffix(&parse_str("Foo").unwrap()).to_string(),
            "type_466f6f"
        );
        for (left, right) in [("用户", "订单"), ("nested::Owner", "nested____Owner")] {
            assert_ne!(
                type_name_suffix(&parse_str(left).unwrap()),
                type_name_suffix(&parse_str(right).unwrap())
            );
        }
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
        // ...while the generated helper fns use the injective encoded suffix.
        let suffix = type_name_suffix(&parse_str("a::b::Foo").unwrap());
        assert!(text.contains(&format!("__macro_gen_oapi_endpoint_type_id_{suffix}")));
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

#[cfg(test)]
mod conditional_diagnostic_tests {
    #[test]
    fn invalid_conditional_endpoint_emits_a_conditional_diagnostic() {
        let input = syn::parse_quote! {
            impl Service {
                #[cfg(all())]
                fn handle(&self, _invalid: &[u32]) {}
            }
        };
        let generated = super::generate(Default::default(), input)
            .unwrap()
            .to_string();
        assert!(generated.contains("compile_error"));
        assert!(generated.contains("the inputs parameters must be Request"));
        assert!(generated.contains("cfg (all ())"));
    }
}

pub(crate) struct CraftArgs<'a> {
    dispatch: Ident,
    creator: Ident,
    request_lifetime: syn::Lifetime,
    method: Ident,
    reserved: TokenStream,
    attr: EndpointAttr<'a>,
}

impl syn::parse::Parse for CraftArgs<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let dispatch = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let creator = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let request_lifetime = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let method = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let reserved;
        syn::parenthesized!(reserved in input);
        let reserved = reserved.parse()?;
        input.parse::<syn::Token![;]>()?;
        Ok(Self {
            dispatch,
            creator,
            request_lifetime,
            method,
            reserved,
            attr: input.parse()?,
        })
    }
}

pub(crate) fn craft_helper(
    mut args: CraftArgs,
    mut helper: syn::ItemFn,
) -> syn::Result<TokenStream> {
    let salvo = crate::salvo_crate();
    let oapi = crate::oapi_crate();
    let reserved_tokens = &args.reserved;
    let mut reserved = reserved_identifiers(quote!(#helper #reserved_tokens));
    let endpoint_type_id = fresh_binding("__salvo_endpoint_type_id", &mut reserved);
    let (hfn, modifiers) = handle_fn(&salvo, &oapi, &helper.sig, &reserved)?;
    args.attr.doc_comments = Some(CommentAttributes::from_attributes(&helper.attrs).0);
    args.attr.deprecated = args.attr.deprecated.or_else(|| {
        helper
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("deprecated"))
            .then_some(true)
    });
    let creator = metadata_inner(
        &salvo,
        &oapi,
        &args.attr,
        &helper.sig.ident,
        &quote!(Self),
        modifiers,
        &[],
        Some((&args.creator, &args.method, &endpoint_type_id)),
    )?;
    let mut creator: syn::ImplItemFn = syn::parse2(creator)?;
    creator.sig.generics.where_clause = helper.sig.generics.where_clause.clone();
    creator
        .attrs
        .push(syn::parse_quote!(#[allow(non_snake_case)]));
    let mut dispatch: syn::ImplItemFn = syn::parse2(hfn)?;
    let lifetime = &args.request_lifetime;
    dispatch.sig.ident = args.dispatch;
    dispatch
        .attrs
        .push(syn::parse_quote!(#[allow(non_snake_case)]));
    dispatch.sig.asyncness = None;
    dispatch.sig.inputs = dispatch.sig.inputs.into_iter().skip(1).collect();
    for input in &mut dispatch.sig.inputs {
        if let syn::FnArg::Typed(input) = input
            && let Type::Reference(reference) = input.ty.as_mut()
        {
            reference.lifetime = Some(lifetime.clone());
        }
    }
    dispatch.sig.generics = syn::parse_quote!(<#lifetime>);
    dispatch.sig.generics.where_clause = helper.sig.generics.where_clause.clone();
    dispatch.sig.output = syn::parse_quote!(-> ::std::pin::Pin<::std::boxed::Box<
        dyn ::std::future::Future<Output = ()> + ::std::marker::Send + #lifetime
    >>);
    let body = dispatch.block;
    dispatch.block = syn::parse_quote!({ ::std::boxed::Box::pin(async move #body) });
    helper
        .attrs
        .retain(|attr| !attr.path().is_ident("deprecated"));
    Ok(quote! { #helper #dispatch #creator })
}
