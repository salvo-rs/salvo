use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use proc_quote::quote;
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, AttributeArgs, FnArg, Ident, ItemFn, Meta, NestedMeta, ReturnType};

pub(crate) enum InputType {
    Request,
    Depot,
    Response,
    FlowCtrl,
    UnKnow,
    NoReferenceArg,
}

// https://github.com/bkchr/proc-macro-crate/issues/14
#[inline]
pub(crate) fn salvo_crate(internal: bool) -> syn::Ident {
    if internal {
        return Ident::new("crate", Span::call_site());
    }
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

#[inline]
pub(crate) fn parse_input_type(input: &FnArg) -> InputType {
    if let FnArg::Typed(p) = input {
        if let syn::Type::Reference(ty) = &*p.ty {
            if let syn::Type::Path(nty) = &*ty.elem {
                // the last ident for path type is the real type
                // such as:
                // `::std::vec::Vec` is `Vec`
                // `Vec` is `Vec`
                let ident = &nty.path.segments.last().unwrap().ident;
                if ident == "Request" {
                    InputType::Request
                } else if ident == "Response" {
                    InputType::Response
                } else if ident == "Depot" {
                    InputType::Depot
                } else if ident == "FlowCtrl" {
                    InputType::FlowCtrl
                } else {
                    InputType::UnKnow
                }
            } else {
                InputType::UnKnow
            }
        } else {
            // like owned type or other type
            InputType::NoReferenceArg
        }
    } else {
        // like self on fn
        InputType::UnKnow
    }
}
