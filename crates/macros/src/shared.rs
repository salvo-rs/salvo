use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::Span;
use syn::{FnArg, Ident, PatType, Receiver, Type, TypePath};

#[allow(dead_code)]
pub(crate) enum InputType<'a> {
    Request(&'a PatType),
    Depot(&'a PatType),
    Response(&'a PatType),
    FlowCtrl(&'a PatType),
    ConnCtrl(&'a PatType),
    Unknown,
    Receiver(&'a Receiver),
    NoReference(&'a PatType),
}

pub(crate) fn salvo_crate() -> syn::Ident {
    match crate_name("salvo") {
        Ok(salvo) => match salvo {
            FoundCrate::Itself => Ident::new("salvo", Span::call_site()),
            FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
        },
        Err(_) => match crate_name("salvo_core") {
            Ok(salvo) => match salvo {
                FoundCrate::Itself => Ident::new("salvo_core", Span::call_site()),
                FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
            },
            Err(_) => Ident::new("salvo", Span::call_site()),
        },
    }
}

pub(crate) fn parse_input_type(input: &FnArg) -> InputType<'_> {
    if let FnArg::Typed(p) = input {
        if let Type::Reference(ty) = &*p.ty {
            if let syn::Type::Path(nty) = &*ty.elem {
                // the last ident for path type is the real type
                // such as:
                // `::std::vec::Vec` is `Vec`
                // `Vec` is `Vec`
                let ident = &nty
                    .path
                    .segments
                    .last()
                    .expect("path segment should exists")
                    .ident;
                if ident == "Request" {
                    InputType::Request(p)
                } else if ident == "Response" {
                    InputType::Response(p)
                } else if ident == "Depot" {
                    InputType::Depot(p)
                } else if ident == "FlowCtrl" {
                    InputType::FlowCtrl(p)
                } else if ident == "ConnCtrl" {
                    InputType::ConnCtrl(p)
                } else {
                    InputType::Unknown
                }
            } else {
                InputType::Unknown
            }
        } else {
            InputType::NoReference(p)
        }
    } else if let FnArg::Receiver(r) = input {
        InputType::Receiver(r)
    } else {
        // like self on fn
        InputType::Unknown
    }
}

/// Replace every named lifetime in a type path with the elided `'_`, leaving
/// `'static` intact, so the type can be used in an `Extractible` bound without
/// carrying caller lifetimes. Operates on the AST via [`VisitMut`] rather than
/// re-parsing a stringified type, which avoids a per-call regex compile, the
/// previous incorrect rewriting of `'static`, and the parse `expect` panic.
pub(crate) fn omit_type_path_lifetimes(ty_path: &TypePath) -> TypePath {
    struct ElideLifetimes {
        elided: syn::Lifetime,
    }
    impl syn::visit_mut::VisitMut for ElideLifetimes {
        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            if lifetime.ident != "static" {
                *lifetime = self.elided.clone();
            }
        }
        fn visit_macro_mut(&mut self, mac: &mut syn::Macro) {
            // `VisitMut` does not parse macro bodies, so a lifetime carried inside a
            // type macro (e.g. `JsonBody<borrowed!('a)>`) would otherwise survive.
            // Rewrite the raw tokens so it is elided like any other.
            let tokens = std::mem::take(&mut mac.tokens);
            mac.tokens = elide_lifetimes_in_tokens(tokens, &self.elided);
        }
    }
    let mut ty_path = ty_path.clone();
    syn::visit_mut::VisitMut::visit_type_path_mut(
        &mut ElideLifetimes {
            elided: syn::parse_quote!('_),
        },
        &mut ty_path,
    );
    ty_path
}

/// Rewrite lifetime tokens (`'name`, except `'static`) to the elided `'_` inside a
/// raw token stream, recursing through groups. Used for macro bodies that
/// [`VisitMut`] does not descend into.
fn elide_lifetimes_in_tokens(
    tokens: proc_macro2::TokenStream,
    elided: &syn::Lifetime,
) -> proc_macro2::TokenStream {
    use proc_macro2::{Group, TokenStream, TokenTree};
    use quote::ToTokens;

    let mut out = TokenStream::new();
    let mut iter = tokens.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Group(group) => {
                let inner = elide_lifetimes_in_tokens(group.stream(), elided);
                let mut new_group = Group::new(group.delimiter(), inner);
                new_group.set_span(group.span());
                out.extend([TokenTree::Group(new_group)]);
            }
            TokenTree::Punct(punct) if punct.as_char() == '\'' => {
                // A lifetime is an apostrophe immediately followed by an identifier.
                if let Some(TokenTree::Ident(ident)) = iter.peek()
                    && *ident != "static"
                {
                    elided.to_tokens(&mut out);
                    iter.next(); // consume the lifetime name
                    continue;
                }
                out.extend([TokenTree::Punct(punct)]);
            }
            other => out.extend([other]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;
    use syn::TypePath;

    use super::omit_type_path_lifetimes;

    fn elide(src: &str) -> String {
        let ty: TypePath = syn::parse_str(src).unwrap();
        omit_type_path_lifetimes(&ty)
            .into_token_stream()
            .to_string()
    }

    #[test]
    fn omit_replaces_named_lifetimes_but_keeps_static() {
        assert_eq!(elide("QueryParam<'a, T>"), "QueryParam < '_ , T >");
        assert_eq!(elide("Cow<'static, str>"), "Cow < 'static , str >");
        assert_eq!(elide("Foo<'a, 'static, T>"), "Foo < '_ , 'static , T >");
        assert_eq!(elide("Vec<String>"), "Vec < String >");
    }

    #[test]
    fn omit_rewrites_lifetimes_inside_type_macros() {
        // `VisitMut` does not descend into macro bodies, so the raw tokens are
        // rewritten directly; `'static` is still preserved.
        let out = elide("JsonBody<borrowed!('a, 'static)>");
        assert!(out.contains("'_"), "{out}");
        assert!(out.contains("'static"), "{out}");
        assert!(!out.contains("'a"), "{out}");
    }
}

/// Keep only configuration effects when guarding generated sibling items.
/// Rust, rather than the proc-macro host, evaluates every predicate.
pub(crate) fn cfg_attributes(attributes: &[syn::Attribute]) -> syn::Result<Vec<syn::Attribute>> {
    fn filter(meta: &syn::Meta) -> syn::Result<Option<syn::Meta>> {
        if meta.path().is_ident("cfg") {
            return Ok(Some(meta.clone()));
        }
        if let syn::Meta::List(list) = meta
            && list.path.is_ident("cfg_attr")
        {
            let (predicate, arguments) =
                list.parse_args_with(|input: syn::parse::ParseStream| {
                    let mut predicate = proc_macro2::TokenStream::new();
                    while !input.is_empty() && !input.peek(syn::Token![,]) {
                        predicate.extend([input.parse::<proc_macro2::TokenTree>()?]);
                    }
                    if predicate.is_empty() {
                        return Err(input.error("cfg_attr requires a predicate"));
                    }
                    input.parse::<syn::Token![,]>()?;
                    let arguments =
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(
                            input,
                        )?;
                    Ok((predicate, arguments))
                })?;
            let conditional = arguments
                .into_iter()
                .map(|meta| filter(&meta))
                .collect::<syn::Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            return Ok((!conditional.is_empty())
                .then(|| syn::parse_quote!(cfg_attr(#predicate, #(#conditional),*))));
        }
        Ok(None)
    }
    attributes
        .iter()
        .filter_map(|attribute| match filter(&attribute.meta) {
            Ok(Some(meta)) => {
                let mut attribute = attribute.clone();
                attribute.meta = meta;
                Some(Ok(attribute))
            }
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

#[cfg(test)]
mod cfg_attribute_tests {
    use quote::quote;

    #[test]
    fn nested_cfg_attr_keeps_conditions_without_unrelated_attributes() {
        let input = syn::parse_quote!(#[cfg_attr(feature = "outer", inline, cfg_attr(feature = "inner", cfg(any()), allow(dead_code)))]);
        let actual = super::cfg_attributes(&[input]).unwrap();
        let expected: syn::Attribute = syn::parse_quote!(#[cfg_attr(feature = "outer", cfg_attr(feature = "inner", cfg(any())))]);
        assert_eq!(
            quote!(#(#actual)*).to_string(),
            quote!(#expected).to_string()
        );
        let unrelated = syn::parse_quote!(#[cfg_attr(feature = "outer", inline)]);
        assert!(super::cfg_attributes(&[unrelated]).unwrap().is_empty());
        let boolean = syn::parse_quote!(#[cfg_attr(true, cfg(false))]);
        let actual = super::cfg_attributes(&[boolean]).unwrap();
        assert_eq!(
            quote!(#(#actual)*).to_string(),
            quote!(#[cfg_attr(true, cfg(false))]).to_string()
        );
    }
}
