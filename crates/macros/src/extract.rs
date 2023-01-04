use cruet::Inflector;
use darling::{FromDeriveInput, FromField, FromMeta};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Attribute, DeriveInput, Error, Generics, Lit, Meta, NestedMeta, Type};

use crate::shared::{is_internal, omit_type_path_lifetimes, salvo_crate};

// #[derive(Debug)]
struct Field {
    ident: Option<Ident>,
    // attrs: Vec<Attribute>,
    ty: Type,
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

impl FromField for Field {
    fn from_field(field: &syn::Field) -> darling::Result<Self> {
        let ident = field.ident.clone();
        let attrs = field.attrs.clone();
        let sources = parse_sources(&attrs, "source")?;
        Ok(Self {
            ident,
            // attrs,
            ty: field.ty.clone(),
            sources,
            aliases: parse_aliases(&field.attrs)?,
            rename: parse_rename(&field.attrs)?,
        })
    }
}

struct ExtractibleArgs {
    ident: Ident,
    generics: Generics,
    fields: Vec<Field>,

    internal: bool,

    default_sources: Vec<RawSource>,
    rename_all: Option<String>,
}

impl FromDeriveInput for ExtractibleArgs {
    fn from_derive_input(input: &DeriveInput) -> darling::Result<Self> {
        let ident = input.ident.clone();
        let generics = input.generics.clone();
        let attrs = input.attrs.clone();
        let default_sources = parse_sources(&attrs, "default_source")?;
        let data = match &input.data {
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
                    if is_internal(list.nested.iter()) {
                        internal = true;
                        break;
                    }
                }
            }
        }
        Ok(Self {
            ident,
            generics,
            fields,
            internal,
            default_sources,
            rename_all: parse_rename_rule(&input.attrs)?,
        })
    }
}

static RENAME_RULES: &[(&str, &str)] = &[
    ("lowercase", "LowerCase"),
    ("UPPERCASE", "UpperCase"),
    ("PascalCase", "PascalCase"),
    ("camelCase", "CamelCase"),
    ("snake_case", "SnakeCase"),
    ("SCREAMING_SNAKE_CASE", "ScreamingSnakeCase"),
    ("kebab-case", "KebabCase"),
    ("SCREAMING-KEBAB-CASE", "ScreamingKebabCase"),
];
fn metadata_rename_rule(salvo: &Ident, input: &str) -> Result<TokenStream, Error> {
    let mut rule = None;
    for (name, value) in RENAME_RULES {
        if input == *name {
            rule = Some(*value);
        }
    }
    match rule {
        Some(rule) => {
            let rule = Ident::new(rule, Span::call_site());
            Ok(quote! {
                #salvo::extract::metadata::RenameRule::#rule
            })
        }
        None => {
            Err(Error::new_spanned(
                input,
                "Invalid rename rule, valid rules are: lowercase, UPPERCASE, PascalCase, camelCase, snake_case, SCREAMING_SNAKE_CASE, kebab-case, SCREAMING-KEBAB-CASE",
            ))
        }
    }
}
fn metadata_source(salvo: &Ident, source: &RawSource) -> TokenStream {
    let from = Ident::new(&source.from.to_pascal_case(), Span::call_site());
    let format = if source.format.to_lowercase() == "multimap" {
        Ident::new("MultiMap", Span::call_site())
    } else {
        Ident::new(&source.format.to_pascal_case(), Span::call_site())
    };
    let from = quote! {
        #salvo::extract::metadata::SourceFrom::#from
    };
    let format = quote! {
        #salvo::extract::metadata::SourceFormat::#format
    };
    quote! {
        #salvo::extract::metadata::Source::new(#from, #format)
    }
}

pub(crate) fn generate(args: DeriveInput) -> Result<TokenStream, Error> {
    let mut args: ExtractibleArgs = ExtractibleArgs::from_derive_input(&args)?;
    let salvo = salvo_crate(args.internal);
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();

    let name = &args.ident;
    let mut default_sources = Vec::new();
    let mut fields = Vec::new();

    for source in &args.default_sources {
        let source = metadata_source(&salvo, source);
        default_sources.push(quote! {
            metadata = metadata.add_default_source(#source);
        });
    }
    let rename_all = if let Some(rename_all) = &args.rename_all {
        let rename = metadata_rename_rule(&salvo, rename_all)?;
        Some(quote! {
            metadata = metadata.rename_all(#rename);
        })
    } else {
        None
    };

    for field in &mut args.fields {
        let field_ident = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new_spanned(name, "All fields must be named."))?
            .to_string();

        let mut sources = Vec::with_capacity(field.sources.len());
        let mut nested_metadata = None;
        for source in &field.sources {
            let from = &source.from;
            if from == "request" {
                if let Type::Path(ty) = &field.ty {
                    let ty = omit_type_path_lifetimes(ty);
                    nested_metadata = Some(quote! {
                        field = field.metadata(<#ty as #salvo::extract::Extractible>::metadata());
                    });
                } else {
                    return Err(Error::new_spanned(name, "Invalid type for request source."));
                }
            }
            let source = metadata_source(&salvo, source);
            sources.push(quote! {
                field = field.add_source(#source);
            });
        }
        if nested_metadata.is_some() && field.sources.len() > 1 {
            return Err(Error::new_spanned(name, "Only one source can be from request."));
        }
        let aliases = field.aliases.iter().map(|alias| {
            quote! {
                field = field.add_alias(#alias);
            }
        });
        let rename = field.rename.as_ref().map(|rename| {
            quote! {
                field = field.rename(#rename);
            }
        });
        fields.push(quote! {
            let mut field = #salvo::extract::metadata::Field::new(#field_ident);
            #nested_metadata
            #(#sources)*
            #(#aliases)*
            #rename
            metadata = metadata.add_field(field);
        });
    }

    let sv = format_ident!("__salvo_extract_{}", name);
    let mt = name.to_string();
    let imp_code = if args.generics.lifetimes().next().is_none() {
        let de_life_def = syn::parse_str("'de").unwrap();
        let mut generics = args.generics.clone();
        generics.params.insert(0, de_life_def);
        let impl_generics_de = generics.split_for_impl().0;
        quote! {
            impl #impl_generics_de #salvo::extract::Extractible<'de> for #name #ty_generics #where_clause {
                fn metadata() ->  &'static #salvo::extract::Metadata {
                    &*#sv
                }
            }
        }
    } else {
        quote! {
            impl #impl_generics #salvo::extract::Extractible #impl_generics for #name #ty_generics #where_clause {
                fn metadata() ->  &'static #salvo::extract::Metadata {
                    &*#sv
                }
            }
        }
    };
    let code = quote! {
        #[allow(non_upper_case_globals)]
        static #sv: #salvo::__private::once_cell::sync::Lazy<#salvo::extract::Metadata> = #salvo::__private::once_cell::sync::Lazy::new(||{
            let mut metadata = #salvo::extract::Metadata::new(#mt);
            #(
                #default_sources
            )*
            #rename_all
            #(
                #fields
            )*
            metadata
        });
        #imp_code
    };
    Ok(code)
}

fn parse_rename(attrs: &[syn::Attribute]) -> darling::Result<Option<String>> {
    for attr in attrs {
        if attr.path.is_ident("extract") {
            if let Meta::List(list) = attr.parse_meta()? {
                for meta in list.nested.iter() {
                    if let NestedMeta::Meta(Meta::NameValue(item)) = meta {
                        if item.path.is_ident("rename") {
                            if let Lit::Str(lit) = &item.lit {
                                return Ok(Some(lit.value()));
                            } else {
                                return Err(darling::Error::custom(format!("invalid rename: {item:?}")));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}

fn parse_rename_rule(attrs: &[syn::Attribute]) -> darling::Result<Option<String>> {
    for attr in attrs {
        if attr.path.is_ident("extract") {
            if let Meta::List(list) = attr.parse_meta()? {
                for meta in list.nested.iter() {
                    if let NestedMeta::Meta(Meta::NameValue(item)) = meta {
                        if item.path.is_ident("rename_all") {
                            if let Lit::Str(lit) = &item.lit {
                                return Ok(Some(lit.value()));
                            } else {
                                return Err(darling::Error::custom(format!("invalid alias: {item:?}")));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}

fn parse_aliases(attrs: &[syn::Attribute]) -> darling::Result<Vec<String>> {
    let mut aliases = Vec::new();
    for attr in attrs {
        if attr.path.is_ident("extract") {
            if let Meta::List(list) = attr.parse_meta()? {
                for meta in list.nested.iter() {
                    if let NestedMeta::Meta(Meta::NameValue(item)) = meta {
                        if item.path.is_ident("alias") {
                            if let Lit::Str(lit) = &item.lit {
                                aliases.push(lit.value());
                            } else {
                                return Err(darling::Error::custom(format!("invalid alias: {item:?}")));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(aliases)
}

fn parse_sources(attrs: &[Attribute], key: &str) -> darling::Result<Vec<RawSource>> {
    let mut sources = Vec::with_capacity(4);
    for attr in attrs {
        if attr.path.is_ident("extract") {
            if let Meta::List(list) = attr.parse_meta()? {
                for meta in list.nested.iter() {
                    if matches!(meta, NestedMeta::Meta(Meta::List(item)) if item.path.is_ident(key)) {
                        let mut source: RawSource = FromMeta::from_nested_meta(meta)?;
                        if source.format.is_empty() {
                            if source.from == "request" {
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
                            return Err(darling::Error::custom(
                                "source format must be `request` for `request` sources",
                            ));
                        }
                        sources.push(source);
                    }
                }
            }
        }
    }
    Ok(sources)
}
