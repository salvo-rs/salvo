use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::ext::IdentExt;
use syn::{Ident, ImplItem, Item, Pat, ReturnType, Signature, Type};

use crate::shared::*;

pub(crate) fn generate(input: Item) -> syn::Result<TokenStream> {
    let salvo = salvo_crate();
    let reserved = reserved_identifiers(input.to_token_stream());
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

            let hfn = handle_fn(&salvo, sig, &reserved)?;
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
            Ok(quote! { #sdef #implementations })
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
                match handle_fn(&salvo, &method.sig, &reserved) {
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
    sig: &Signature,
    reserved: &::std::collections::HashSet<String>,
) -> syn::Result<TokenStream> {
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
                if let (Pat::Ident(pat_ident), Type::Path(ty)) =
                    (&*pat.pat, unwrapped_type(&pat.ty))
                {
                    let id = pat_ident.ident.clone();
                    call_args.push(id.clone());
                    let ty = omit_type_path_lifetimes(ty);
                    // Derive the extractor arg name from the parsed identifier
                    // directly. The previous code re-stringified the whole pattern
                    // and rebuilt it with `Ident::new`, which panics on a raw
                    // identifier such as `r#type`. `unraw` drops a leading `r#`.
                    let idv = pat_ident.ident.unraw().to_string();
                    let idv = idv.trim_start_matches('_');

                    extract_ts.push(quote!{
                        let #id: #ty = match <#ty as #salvo::Extractible>::extract_with_arg(#__macro_gen_req, #__macro_gen_depot, #idv).await {
                            Ok(#__macro_gen_data) => #__macro_gen_data,
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

    match sig.output {
        ReturnType::Default => {
            if sig.asyncness.is_none() {
                Ok(quote! {
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        Self::#name(#(#call_args),*)
                    }
                })
            } else {
                Ok(quote! {
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
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
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*), #__macro_gen_req, #__macro_gen_depot, #__macro_gen_res).await;
                    }
                })
            } else {
                Ok(quote! {
                    async fn handle(&self, #__macro_gen_req: &mut #salvo::Request, #__macro_gen_depot: &mut #salvo::Depot, #__macro_gen_res: &mut #salvo::Response, #__macro_gen_ctrl: &mut #salvo::FlowCtrl) {
                        #conn_init
                        #(#extract_ts)*
                        #salvo::Writer::write(Self::#name(#(#call_args),*).await, #__macro_gen_req, #__macro_gen_depot, #__macro_gen_res).await;
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

pub(crate) struct CraftArgs {
    pub(crate) dispatch: Ident,
    pub(crate) request_lifetime: syn::Lifetime,
    reserved: TokenStream,
}

impl syn::parse::Parse for CraftArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let dispatch = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let request_lifetime = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let reserved;
        syn::parenthesized!(reserved in input);
        Ok(Self {
            dispatch,
            request_lifetime,
            reserved: reserved.parse()?,
        })
    }
}

pub(crate) fn craft_helper(args: CraftArgs, mut helper: syn::ItemFn) -> syn::Result<TokenStream> {
    let salvo = salvo_crate();
    let reserved_tokens = &args.reserved;
    let reserved = reserved_identifiers(quote!(#helper #reserved_tokens));
    let hfn = handle_fn(&salvo, &helper.sig, &reserved)?;
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
    Ok(quote! { #helper #dispatch })
}
