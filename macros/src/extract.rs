use darling::{ast::Data, util::Ignored, FromDeriveInput, FromField, FromMeta};
use proc_macro2::{Ident, TokenStream};
use proc_quote::quote;
use syn::{ext::IdentExt, Attribute, DeriveInput, Error, Generics, Path, Type};

use crate::shared::salvo_crate;

#[derive(FromField)]
#[darling(attributes(extract))]
struct Field {
    ident: Option<Ident>,
    ty: Type,
    attrs: Vec<Attribute>,

    #[darling(default)]
    sources: Sources,
}

#[derive(FromDeriveInput)]
#[darling(attributes(extract))]
struct ExtractibleArgs {
    ident: Ident,
    generics: Generics,
    attrs: Vec<Attribute>,
    data: Data<Ignored, Field>,

    #[darling(default)]
    internal: bool,

    #[darling(default)]
    default_sources: Sources,
}

#[derive(FromMeta)]
struct Source {
    from: String,
    format: Option<String>,
}

#[derive(Default)]
struct Sources(Vec<Source>);

impl FromMeta for Sources {

}

pub(crate) fn generate(args: DeriveInput) -> Result<TokenStream, Error> {
    let args: ExtractibleArgs = ExtractibleArgs::from_derive_input(&args)?;
    let salvo = salvo_crate(args.internal);
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();
    let ident = &args.ident;
    let s = match &args.data {
        Data::Struct(s) => s,
        _ => {
            return Err(Error::new_spanned(ident, "Extractible can only be applied to an struct.").into());
        }
    };
    let mut default_sources = Vec::new();
    let mut fields = Vec::new();

    for source in &args.default_sources.0 {
        let from = &source.from;
        let format = source.format.as_deref().unwrap_or("multimap");
        default_sources.push(quote! {
            metadata.add_default_source(#salvo::extract::metadata::Source::new(#from.parse().unwrap(), #format.parse().unwrap()))
        });
    }

    for field in &s.fields {
        let field_ident = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new_spanned(&ident, "All fields must be named."))?;
        let field_ty = &field.ty;

        let mut sources = Vec::with_capacity(field.sources.0.len());
        for source in &field.sources.0 {
            let from = &source.from;
            let format = source.format.as_deref().unwrap_or("multimap");
            sources.push(quote! {
                #salvo::extract::metadata::Source::new(#from.parse().unwrap(), #format.parse().unwrap())
            });
        }
        fields.push(quote! {
            metadata.add_field(#salvo::extract::metadata::Field::with_sources(#field_ident, #field_ty, vec![#(#sources,)*]))
        });
    }

    Ok(quote! {
        static #ident: #salvo::extract::Metadata = #salvo::__private::once_cell::sync::Lazy::new(||{
            let mut metadata = Metadata::new(#ident, #salvo::extract::metadata::DataKind::Struct);
            #(
                #default_sources
            )*
            #(
                #fields
            )*
            metadata
        });
        impl #impl_generics #salvo::extract::Extractible for #ident #ty_generics #where_clause {
            fn metadata() -> &'static #salvo::extract::Metadata {
                &&*#ident
            }
        }
    })
}
