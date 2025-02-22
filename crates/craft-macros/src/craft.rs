use crate::utils::salvo_crate;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use regex::Regex;
use syn::parse::Parser;
use syn::{
    AngleBracketedGenericArguments, Attribute, FnArg, Generics, Ident, ImplItem, ImplItemFn, Item,
    PathArguments, Token, Type, TypePath, parse_quote,
};

pub(crate) fn generate(input: Item) -> syn::Result<TokenStream> {
    match input {
        Item::Impl(mut item_impl) => {
            for item in &mut item_impl.items {
                if let ImplItem::Fn(method) = item {
                    rewrite_method(
                        item_impl.generics.clone(),
                        item_impl.self_ty.clone(),
                        method,
                    )?;
                }
            }
            Ok(item_impl.into_token_stream())
        }
        Item::Fn(_) => Ok(input.into_token_stream()),
        _ => Err(syn::Error::new_spanned(
            input,
            "#[craft] must added to `impl`",
        )),
    }
}

const REGEX_STR: &str = r#"(?s)#\s*\[\s*(::)?\s*([A-Za-z_][A-Za-z0-9_]*\s*::\s*)*\s*craft\s*\(\s*(?P<name>handler|endpoint)\s*(?P<content>\(.*\))?\s*\)\s*\]"#;

fn take_method_macro(item_fn: &mut ImplItemFn) -> syn::Result<Option<Attribute>> {
    let mut index: Option<usize> = None;
    let mut new_attr: Option<Attribute> = None;
    let re = Regex::new(REGEX_STR).expect("regex compile should not fail");
    for (idx, attr) in &mut item_fn.attrs.iter().enumerate() {
        if !(match attr.path().segments.last() {
            Some(segment) => segment.ident == "craft",
            None => false,
        }) {
            continue;
        }
        let attr_str = attr.to_token_stream().to_string().trim().to_owned();
        if let Some(caps) = re.captures(&attr_str) {
            if let Some(name) = caps.name("name") {
                let name = name.as_str();
                let content = caps
                    .name("content")
                    .map(|c| c.as_str().to_string())
                    .unwrap_or_default();
                let ts: TokenStream = match name {
                    "handler" => format!("#[{}::{name}{content}]", salvo_crate()).parse()?,
                    "endpoint" => format!("#[{}::oapi::{name}{content}]", salvo_crate()).parse()?,
                    _ => {
                        unreachable!()
                    }
                };
                new_attr = Attribute::parse_outer.parse2(ts)?.into_iter().next();
                index = Some(idx);
                continue;
            }
        }
        return Err(syn::Error::new_spanned(
            item_fn,
            format!(
                "The attribute macro `{attr_str}` on a method must be filled with sub-attributes, such as '#[craft(handler)]', '#[craft(endpoint)]', or '#[craft(endpoint(...))]'."
            ),
        ));
    }
    if let Some(index) = index {
        item_fn.attrs.remove(index);
        return Ok(new_attr);
    }
    Ok(None)
}

enum FnReceiver {
    None,
    Ref,
    Arc,
}

impl FnReceiver {
    fn from_method(method: &ImplItemFn) -> syn::Result<Self> {
        let Some(recv) = method.sig.receiver() else {
            return Ok(Self::None);
        };
        let ty = recv.ty.to_token_stream().to_string().replace(" ", "");
        match ty.as_str() {
            "&Self" => Ok(Self::Ref),
            "Arc<Self>" | "&Arc<Self>" => Ok(Self::Arc),
            _ => {
                if ty.ends_with("::Arc<Self>") {
                    Ok(Self::Arc)
                } else {
                    Err(syn::Error::new_spanned(
                        method,
                        "#[craft] method receiver must be '&self', 'Arc<Self>' or '&Arc<Self>'",
                    ))
                }
            }
        }
    }
}

fn rewrite_method(
    mut impl_generics: Generics,
    self_ty: Box<Type>,
    method: &mut ImplItemFn,
) -> syn::Result<()> {
    let Some(macro_attr) = take_method_macro(method)? else {
        return Ok(());
    };
    method.sig.asyncness = Some(Token![async](Span::call_site()));
    let salvo = salvo_crate();
    let handler = quote!(#salvo::Handler);
    let method_name = method.sig.ident.clone();
    let vis = method.vis.clone();
    let mut attrs = method.attrs.clone();
    let mut new_method: ImplItemFn = match FnReceiver::from_method(method)? {
        FnReceiver::None => {
            method.attrs.push(macro_attr);
            parse_quote! {
                #vis fn #method_name() -> impl #handler {

                    #method

                    #method_name
                }
            }
        }
        style => {
            let (receiver, output) = match style {
                FnReceiver::Ref => (quote!(&self), quote!(::std::sync::Arc::new(self.clone()))),
                FnReceiver::Arc => (quote!(self: &::std::sync::Arc<Self>), quote!(self.clone())),
                _ => unreachable!(),
            };
            method.sig.inputs[0] = FnArg::Receiver(parse_quote!(&self));
            method.sig.ident = Ident::new("handle", Span::call_site());
            let where_clause = impl_generics.make_where_clause().clone();
            let mut angle_bracketed: Option<AngleBracketedGenericArguments> = None;
            if let Type::Path(TypePath { path, .. }) = &*self_ty {
                if let Some(last_segment) = path.segments.last() {
                    if let PathArguments::AngleBracketed(_angle_bracketed) = &last_segment.arguments
                    {
                        // println!("{}", _angle_bracketed.to_token_stream());
                        angle_bracketed = Some(_angle_bracketed.clone());
                    }
                }
            }
            parse_quote! {
                #vis fn #method_name(#receiver) -> impl #handler {
                    #[allow(non_camel_case_types)]
                    pub struct handle #impl_generics(::std::sync::Arc<#self_ty>) #where_clause;
                    use ::std::ops::Deref;
                    impl #impl_generics Deref for handle #angle_bracketed #where_clause{
                        type Target = #self_ty;

                        fn deref(&self) -> &Self::Target {
                            &self.0
                        }
                    }
                    #[allow(unused_imports)]
                    use ::std::ops::Deref as _;
                    #macro_attr
                    impl #impl_generics handle #angle_bracketed #where_clause{
                        #method
                    }
                    handle(#output)
                }
            }
        }
    };
    new_method.attrs.append(&mut attrs);
    *method = new_method;
    // println!("{}", method.to_token_stream());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::REGEX_STR;
    use regex::Regex;

    #[test]
    fn extract_attribute() {
        let re = Regex::new(REGEX_STR).unwrap();

        let texts = vec![
            r###"#[:: craft(endpoint(responses((status_code = 400, description = "[(Wrong)] request parameters."))))]"###,
            r###"#[ xx ::craft(handler())]"###,
            r###"#[::xx::craft(endpoint(simple_text))] "###,
            r###"#[craft(handler)]"###,
        ];
        for text in texts {
            for caps in re.captures_iter(text) {
                println!(
                    "name={}, content={:?}",
                    caps.name("name").unwrap().as_str(),
                    caps.name("content").map(|c| c.as_str().to_owned())
                )
            }
        }
    }
}
