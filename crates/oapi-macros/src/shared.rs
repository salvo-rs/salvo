use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use syn::Ident;

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
