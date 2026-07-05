use syn::punctuated::Punctuated;
use syn::{Attribute, Meta, MetaList, Token};

pub(crate) fn find_nested_list(attr: &Attribute, ident: &str) -> syn::Result<Option<MetaList>> {
    find_nested_list_any(attr, &[ident])
}

/// Like [`find_nested_list`], but matches any of several accepted idents.
///
/// Used to accept both spellings of keys that historically drifted between
/// singular and plural (e.g. `parameter` / `parameters`), so the same key works
/// on both the container and its fields.
pub(crate) fn find_nested_list_any(
    attr: &Attribute,
    idents: &[&str],
) -> syn::Result<Option<MetaList>> {
    let metas = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
    for meta in metas {
        if let Meta::List(meta) = meta
            && idents.iter().any(|ident| meta.path.is_ident(ident))
        {
            return Ok(Some(meta));
        }
    }
    Ok(None)
}

pub(crate) fn has_nested_path(attr: &Attribute, ident: &str, path: &str) -> syn::Result<bool> {
    let Some(list) = find_nested_list(attr, ident)? else {
        return Ok(false);
    };
    let Ok(metas) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
        return Ok(false);
    };
    for meta in metas {
        if let Meta::Path(meta) = meta
            && meta.is_ident(path)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
