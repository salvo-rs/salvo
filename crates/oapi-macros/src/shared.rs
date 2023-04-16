use proc_macro2::{TokenStream, Span};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::PathArguments::AngleBracketed;
use syn::{Attribute, FnArg, Generics, GenericArgument, GenericParam, Ident, Lit, Meta, PatType, Receiver, TypePath};

// https://github.com/bkchr/proc-macro-crate/issues/14
#[inline]
pub(crate) fn root_crate() -> syn::Ident {
    match crate_name("salvo-oapi") {
        Ok(oapi) => match oapi {
            FoundCrate::Itself => Ident::new("crate", Span::call_site()),
            FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
        },
        Err(_) => Ident::new("salvo", Span::call_site()),
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
