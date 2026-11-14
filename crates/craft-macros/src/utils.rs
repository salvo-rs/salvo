use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::Span;
use syn::Ident;

pub(crate) fn oapi_crate() -> syn::Ident {
    match crate_name("salvo-oapi") {
        Ok(FoundCrate::Itself) => Ident::new("salvo_oapi", Span::call_site()),
        Ok(FoundCrate::Name(name)) => Ident::new(&name, Span::call_site()),
        Err(_) => salvo_crate(),
    }
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
