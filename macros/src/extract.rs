use std::vec;

use darling::{ast::Data, util::Ignored, FromDeriveInput, FromField, FromMeta};
use proc_macro2::{Ident, Span, TokenStream};
use proc_quote::quote;
use syn::{ext::IdentExt, Attribute, DeriveInput, Error, Generics, Meta, NestedMeta, Path, Type, GenericParam};

use crate::shared::salvo_crate;

#[derive(FromField, Debug)]
#[darling(attributes(extract), forward_attrs(extract))]
struct Field {
    ident: Option<Ident>,
    ty: Type,
    attrs: Vec<Attribute>,

    #[darling(default, rename="source")]
    sources: Sources,
}

#[derive(FromDeriveInput)]
#[darling(attributes(extract), forward_attrs(extract))]
struct ExtractibleArgs {
    ident: Ident,
    generics: Generics,
    attrs: Vec<Attribute>,
    data: Data<Ignored, Field>,

    #[darling(default)]
    internal: bool,

    #[darling(default)]
    default_sources: Sources,

    default_source: Option<Source>,
}

#[derive(FromMeta, Debug)]
struct Source {
    from: String,
    format: Option<String>,
}

#[derive(Default, Debug)]
struct Sources(Vec<Source>);

impl FromMeta for Sources {
    fn from_list(items: &[NestedMeta]) -> Result<Self, darling::Error> {
        let mut sources = Vec::with_capacity(items.len());
        for item in items {
            if let NestedMeta::Meta(Meta::List(ref item)) = *item {
                let meta = item.nested.iter().cloned().collect::<Vec<syn::NestedMeta>>();
                let source: Source = FromMeta::from_list(&meta).unwrap();
                sources.push(source);
            }
        }

        Ok(Sources(sources))
    }
}

pub(crate) fn generate(mut args: DeriveInput) -> Result<TokenStream, Error> {
    let mut eargs: ExtractibleArgs = ExtractibleArgs::from_derive_input(&args)?;
    println!("b bbbbbb {:#?}", eargs.data);
    let salvo = salvo_crate(eargs.internal);
    let (impl_generics, ty_generics, where_clause) = eargs.generics.split_for_impl();


    let ident = &args.ident;
    let mut s = match eargs.data {
        Data::Struct(s) => s,
        _ => {
            return Err(Error::new_spanned(ident, "Extractible can only be applied to an struct.").into());
        }
    };
    let mut default_sources = Vec::new();
    let mut fields = Vec::new();

    if let Some(source) = eargs.default_source.take() {
       eargs.default_sources.0.push(source);
    }
    for source in &eargs.default_sources.0 {
        let from = &source.from;
        let format = source.format.as_deref().unwrap_or("multimap");
        default_sources.push(quote! {
            metadata = metadata.add_default_source(#salvo::extract::metadata::Source::new(#from.parse().unwrap(), #format.parse().unwrap()));
        });
    }

    for field in &mut s.fields {
        let field_ident = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new_spanned(&ident, "All fields must be named."))?
            .to_string();
        // let field_ty = field.ty.to_string();

        let mut sources = Vec::with_capacity(field.sources.0.len());
        for source in &field.sources.0 {
            let from = &source.from;
            let format = source.format.as_deref().unwrap_or("multimap");
            sources.push(quote! {
                field = field.add_source(#salvo::extract::metadata::Source::new(#from.parse().unwrap(), #format.parse().unwrap()));
            });
        }
        fields.push(quote! {
            let mut field = #salvo::extract::metadata::Field::new(#field_ident, "struct".parse().unwrap());
            #(#sources)*
            metadata = metadata.add_field(field);
        });
    }

    let sv = Ident::new(&format!("__salvo_extract_{}", ident.to_string()), Span::call_site());
    let mt = ident.to_string();
    let imp_code = if args.generics.lifetimes().next().is_none() {
        let de_life_def = syn::parse_str("'de").unwrap();
        args.generics.params.insert(0, de_life_def);
        let impl_generics_de = args.generics.split_for_impl().0;
        quote! {
            impl #impl_generics_de #salvo::extract::Extractible<'de> for #ident #ty_generics #where_clause {
                fn metadata() ->  &'static #salvo::extract::Metadata {
                    &*#sv
                }
            }
        }
    } else {
        quote! {
            impl #impl_generics #salvo::extract::Extractible #impl_generics for #ident #ty_generics #where_clause {
                fn metadata() ->  &'static #salvo::extract::Metadata {
                    &*#sv
                }
            }
        }
    };
    let code = quote! {
        static #sv: #salvo::__private::once_cell::sync::Lazy<#salvo::extract::Metadata> = #salvo::__private::once_cell::sync::Lazy::new(||{
            let mut metadata = #salvo::extract::Metadata::new(#mt, #salvo::extract::metadata::DataKind::Struct);
            #(
                #default_sources
            )*
            #(
                #fields
            )*
            metadata
        });
        #imp_code
    };

    println!("{}", code);

    Ok(code)
}
