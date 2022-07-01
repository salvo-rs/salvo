use std::vec;

use darling::{ast::Data, util::Ignored, FromDeriveInput, FromField, FromMeta};
use proc_macro2::{Ident, Span, TokenStream};
use proc_quote::quote;
use syn::ext::IdentExt;
use syn::{
    Attribute, AttributeArgs, DeriveInput, Error, GenericParam, Generics, Meta, MetaList, NestedMeta, Path, Type,
};

use crate::shared::salvo_crate;

#[derive(Debug)]
struct Field {
    ident: Option<Ident>,
    ty: Type,
    attrs: Vec<Attribute>,

    sources: Vec<RawSource>,
    aliases: Vec<String>,
    rename: Option<String>,
}
#[derive(FromMeta, Debug)]
struct RawSource {
    from: String,
    #[darling(default)]
    format: String,
}

// impl RawSource {
//     fn try_into_source(self) -> Result<Source, Error> {
//         let from: SourceFrom = self.from.as_str().parse().unwrap();
//         let format: SourceFormat = if let Some(format) = self.format {
//            format.as_str().parse().unwrap()
//         } else {
//             if from == SourceFrom::Request {
//                 SourceFormat::Request
//             } else {
//                 SourceFormat::MultiMap
//             }
//         };
//         if from == SourceFrom::Request && format != SourceFormat::Request{
//             return Err(Error::new(
//                 Span::call_site(),
//                 "source format must be `request` for `request` sources",
//             ));
//         }
//         Ok(Source::new(from, format))
// }

fn parse_sources(attrs: &[Attribute], key: &str) -> darling::Result<Vec<RawSource>> {
    let mut sources = Vec::with_capacity(4);
    for attr in attrs {
        if attr.path.is_ident("extract") {
            if let Meta::List(list) = attr.parse_meta()? {
                for meta in list.nested.iter() {
                    if matches!(meta, NestedMeta::Meta(Meta::List(item)) if item.path.is_ident(key)) {
                        let mut source: RawSource = FromMeta::from_nested_meta(meta)?;
                        if source.format.is_empty() {
                            if source.format == "request" {
                                source.format = "request".to_string();
                            } else {
                                source.format = "multimap".to_string();
                            }
                        }
                        if !["request", "param", "query", "header", "body"].contains(&source.from.as_str()) {
                            return Err(darling::Error::custom(format!(
                                "source from is invalid: {}",
                                source.from
                            )));
                        }
                        if !["multimap", "json", "request"].contains(&source.format.as_str()) {
                            return Err(darling::Error::custom(format!(
                                "source format is invalid: {}",
                                source.format
                            )));
                        }
                        if source.from == "request" && source.format != "request" {
                            return Err(darling::Error::custom(format!(
                                "source format must be `request` for `request` sources"
                            )));
                        }
                        sources.push(source);
                    }
                }
            }
        }
    }
    Ok(sources)
}

impl FromField for Field {
    fn from_field(field: &syn::Field) -> darling::Result<Self> {
        let ident = field.ident.clone();
        let ty = field.ty.clone();
        let attrs = field.attrs.clone();
        let sources = parse_sources(&attrs, "source")?;
        Ok(Self {
            ident,
            ty,
            attrs,
            sources,
            aliases: vec![],
            rename: None,
        })
    }
}

struct ExtractibleArgs {
    ident: Ident,
    generics: Generics,
    attrs: Vec<Attribute>,
    fields: Vec<Field>,

    internal: bool,

    default_sources: Vec<RawSource>,
}

impl FromDeriveInput for ExtractibleArgs {
    fn from_derive_input(input: &DeriveInput) -> darling::Result<Self> {
        let ident = input.ident.clone();
        let generics = input.generics.clone();
        let attrs = input.attrs.clone();
        let default_sources = parse_sources(&attrs, "default_source")?;
        let mut data = match &input.data {
            syn::Data::Struct(data) => data,
            _ => {
                return Err(Error::new_spanned(ident, "Extractible can only be applied to an struct.").into());
            }
        };
        let mut fields = Vec::with_capacity(data.fields.len());
        for field in data.fields.iter() {
            fields.push(Field::from_field(field)?);
        }
        let mut internal = false;
        for attr in &attrs {
            if attr.path.is_ident("extract") {
                if let Meta::List(list) = attr.parse_meta()? {
                    for meta in list.nested.iter() {
                        if matches!(meta, NestedMeta::Meta(Meta::Path(item)) if item.is_ident("internal")) {
                            internal = true;
                        }
                    }
                }
                if internal {
                    break;
                }
            }
        }
        Ok(Self {
            ident,
            generics,
            attrs,
            fields,
            internal,
            default_sources,
        })
    }
}

// #[derive(Default, Debug)]
// struct Sources(Vec<Source>);

// impl FromMeta for Sources {
//     fn from_list(items: &[NestedMeta]) -> Result<Self, darling::Error> {
//         // println!("========items: {:#?}", items );
//         let mut sources = Vec::with_capacity(items.len());
//         for item in items {
//             if let NestedMeta::Meta(Meta::List(ref item)) = *item {
//                 let meta = item.nested.iter().cloned().collect::<Vec<syn::NestedMeta>>();
//                 let source: Source = FromMeta::from_list(&meta).unwrap();
//                 sources.push(source);
//             }
//         }

//         Ok(Sources(sources))
//     }
// }

pub(crate) fn generate(mut args: DeriveInput) -> Result<TokenStream, Error> {
    let mut args: ExtractibleArgs = ExtractibleArgs::from_derive_input(&args)?;
    let salvo = salvo_crate(args.internal);
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();

    let ident = &args.ident;
    let mut default_sources = Vec::new();
    let mut fields = Vec::new();

    for source in &args.default_sources {
        let from = &source.from;
        let format = &source.format;
        default_sources.push(quote! {
            metadata = metadata.add_default_source(#salvo::extract::metadata::Source::new(#from.parse().unwrap(), #format.parse().unwrap()));
        });
    }

    for field in &mut args.fields {
        let field_ident = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new_spanned(&ident, "All fields must be named."))?
            .to_string();
        // let field_ty = field.ty.to_string();

        let mut sources = Vec::with_capacity(field.sources.len());
        for source in &field.sources {
            let from = &source.from;
            let format = &source.format;
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
        let mut generics = args.generics.clone();
        generics.params.insert(0, de_life_def);
        let impl_generics_de = generics.split_for_impl().0;
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
