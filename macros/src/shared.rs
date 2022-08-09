use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use syn::PathArguments::AngleBracketed;
use syn::{FnArg, GenericArgument, Ident, Meta, NestedMeta, PatType, Receiver, TypePath};

pub(crate) enum InputType<'a> {
    Request(&'a PatType),
    Depot(&'a PatType),
    Response(&'a PatType),
    FlowCtrl(&'a PatType),
    Unknown,
    Receiver(&'a Receiver),
    NoReference(&'a PatType),
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
                    InputType::Request(p)
                } else if ident == "Response" {
                    InputType::Response(p)
                } else if ident == "Depot" {
                    InputType::Depot(p)
                } else if ident == "FlowCtrl" {
                    InputType::FlowCtrl(p)
                } else {
                    InputType::Unknown
                }
            } else {
                InputType::Unknown
            }
        } else {
            // like owned type or other type
            InputType::NoReference(p)
        }
    } else if let FnArg::Receiver(r) = input {
        InputType::Receiver(r)
    } else {
        // like self on fn
        InputType::Unknown
    }
}

pub(crate) fn omit_type_path_lifetimes(ty_path: &TypePath) -> (TypePath, usize) {
    let mut ty_path = ty_path.clone();
    let mut count = 0;
    for seg in ty_path.path.segments.iter_mut() {
        if let AngleBracketed(ref mut args) = seg.arguments {
            for arg in args.args.iter_mut() {
                if let GenericArgument::Lifetime(lifetime) = arg {
                    lifetime.ident = Ident::new("_", Span::call_site());
                    count += 1;
                }
            }
        }
    }
    (ty_path, count)
}

pub(crate) fn is_internal<'a>(args: impl Iterator<Item = &'a NestedMeta>) -> bool {
    for arg in args {
        if matches!(arg,NestedMeta::Meta(Meta::Path(p)) if p.is_ident("internal")) {
            return true;
        }
    }
    false
}
