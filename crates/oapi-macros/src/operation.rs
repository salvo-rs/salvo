use darling::{ast::Data, util::Ignored, FromDeriveInput, FromField};
use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::ext::IdentExt;
use syn::{Attribute, NestedMeta, Meta, AttributeArgs, DeriveInput, Error, Generics, Item, Path, Type};

use crate::shared::{create_object_name, get_description, optional_literal, root_crate};

#[derive(FromDeriveInput)]
#[darling(attributes(salvo_oapi), forward_attrs(doc))]
struct HandlerArgs {
    ident: Ident,
    generics: Generics,
    attrs: Vec<Attribute>,

    #[darling(default)]
    internal: bool,

    id: Option<String>,

    summary: Option<String>,
    description: Option<String>,
    #[darling(default)]
    deprecated: bool,
    #[darling(default)]
    security: bool,
    // #[darling(default)]
    // external_docs: Option<ExternalDocument>,
}

pub(crate) fn generate(internal: bool, args: &AttributeArgs, input: Item) -> syn::Result<TokenStream> {
    let salvo_oapi = root_crate(internal);
    for arg in args.iter() {
        if let NestedMeta::Meta(Meta::List(arg)) = arg{
            println!("{:#?}", arg);
        }
    }
    // match input {
    //     Item::Fn(mut item_fn) => {}
    //     Item::Impl(item_impl) => {}
    // }
    Ok(quote! {
        #input
    })
}
