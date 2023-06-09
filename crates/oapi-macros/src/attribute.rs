use syn::punctuated::Punctuated;
use syn::{Attribute, Meta, MetaList, Token};

pub(crate) fn find_nested_list(attr: &Attribute, ident: &str) -> syn::Result<Option<MetaList>> {
    let metas = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
    for meta in metas {
        if let Meta::List(meta) = meta {
            if meta.path.is_ident(ident) {
                return Ok(Some(meta));
            }
        }
    }
    Ok(None)
}

pub(crate) fn has_nested_path(attr: &Attribute, ident: &str, path: &str) -> syn::Result<bool> {
    let Some(list) = find_nested_list(attr, ident)? else {
        return Ok(false);
    };
    let Ok(metas) =  list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
        return Ok(false);
    };
    for meta in metas {
        if let Meta::Path(meta) = meta {
            if meta.is_ident(path) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
