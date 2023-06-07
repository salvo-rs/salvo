use cruet::Inflector;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{DeriveInput, Error, Expr, ExprLit, Field, Generics, Lit, Meta, MetaNameValue, Token, Type};

use crate::{attribute, omit_type_path_lifetimes, salvo_crate};

struct FieldInfo {
    ident: Option<Ident>,
    ty: Type,
    sources: Vec<SourceInfo>,
    aliases: Vec<String>,
    rename: Option<String>,
}
impl TryFrom<&Field> for FieldInfo {
    type Error = Error;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        let ident = field.ident.clone();
        let attrs = field.attrs.clone();
        let mut sources: Vec<SourceInfo> = Vec::with_capacity(field.attrs.len());
        let mut aliases = Vec::with_capacity(field.attrs.len());
        let mut rename = None;
        for attr in attrs {
            if attr.path().is_ident("salvo") {
                if let Ok(Some(metas)) = attribute::find_nested_list(&attr, "extract") {
                    let info: ExtractFieldInfo = metas.parse_args()?;
                    sources.extend(info.sources);
                    aliases.extend(info.aliases);
                    if info.rename.is_some() {
                        rename = info.rename;
                    }
                }
            }
        }
        sources.dedup();
        aliases.dedup();
        Ok(Self {
            ident,
            ty: field.ty.clone(),
            sources,
            aliases,
            rename,
        })
    }
}

#[derive(Default, Debug)]
struct ExtractFieldInfo {
    sources: Vec<SourceInfo>,
    aliases: Vec<String>,
    rename: Option<String>,
}
impl Parse for ExtractFieldInfo {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut extract = Self::default();
        while !input.is_empty() {
            let id = input.parse::<syn::Ident>()?;
            if id == "source" {
                let item;
                syn::parenthesized!(item in input);
                extract.sources.push(item.parse::<SourceInfo>()?);
            } else if id == "rename" {
                input.parse::<Token![=]>()?;
                let expr = input.parse::<Expr>()?;
                extract.rename = Some(expr_lit_value(&expr)?);
            } else if id == "alias" {
                input.parse::<Token![=]>()?;
                let expr = input.parse::<Expr>()?;
                extract.aliases.push(expr_lit_value(&expr)?);
            } else {
                return Err(input.error("unexpected attribute"));
            }
            input.parse::<Token![,]>().ok();
        }
        Ok(extract)
    }
}

#[derive(Eq, PartialEq, Debug)]
struct SourceInfo {
    from: String,
    format: String,
}

impl Parse for SourceInfo {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut source = SourceInfo {
            from: "request".to_string(),
            format: "".to_string(),
        };
        let fields: Punctuated<MetaNameValue, Token![,]> = Punctuated::parse_terminated(input)?;
        for field in fields {
            if field.path.is_ident("from") {
                source.from = expr_lit_value(&field.value)?;
            } else if field.path.is_ident("format") {
                source.format = expr_lit_value(&field.value)?;
            } else {
                return Err(input.error("unexpected attribute"));
            }
        }
        if source.format.is_empty() {
            if source.from == "request" {
                source.format = "request".to_string();
            } else {
                source.format = "multimap".to_string();
            }
        }
        if !["request", "param", "query", "header", "body"].contains(&source.from.as_str()) {
            return Err(Error::new(
                input.span(),
                format!("source from is invalid: {}", source.from),
            ));
        }
        if !["multimap", "json", "request"].contains(&source.format.as_str()) {
            return Err(Error::new(
                input.span(),
                format!("source format is invalid: {}", source.format),
            ));
        }
        if source.from == "request" && source.format != "request" {
            return Err(Error::new(
                input.span(),
                "source format must be `request` for `request` sources",
            ));
        }
        Ok(source)
    }
}

struct ExtractibleArgs {
    ident: Ident,
    generics: Generics,
    fields: Vec<FieldInfo>,

    default_sources: Vec<SourceInfo>,
    rename_all: Option<String>,
}

impl ExtractibleArgs {
    fn from_derive_input(input: &DeriveInput) -> syn::Result<Self> {
        let ident = input.ident.clone();
        let generics = input.generics.clone();
        let attrs = input.attrs.clone();
        let data = match &input.data {
            syn::Data::Struct(data) => data,
            _ => {
                return Err(Error::new_spanned(
                    ident,
                    "extractible can only be applied to an struct.",
                ));
            }
        };
        let mut fields = Vec::with_capacity(data.fields.len());
        for field in data.fields.iter() {
            fields.push(field.try_into()?);
        }
        let mut default_sources = Vec::new();
        let mut rename_all = None;
        for attr in &attrs {
            if attr.path().is_ident("salvo") {
                if let Ok(Some(metas)) = attribute::find_nested_list(attr, "extract") {
                    let nested = metas.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)?;
                    for meta in nested {
                        match meta {
                            Meta::List(meta) => {
                                if meta.path.is_ident("default_source") {
                                    default_sources.push(meta.parse_args()?);
                                }
                            }
                            Meta::NameValue(meta) => {
                                if meta.path.is_ident("rename_all") {
                                    rename_all = Some(expr_lit_value(&meta.value)?);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(Self {
            ident,
            generics,
            fields,
            default_sources,
            rename_all,
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
fn metadata_source(salvo: &Ident, source: &SourceInfo) -> TokenStream {
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
    let salvo = salvo_crate();
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

    let sv: Ident = format_ident!("__salvo_extract_{}", name);
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

fn expr_lit_value(expr: &Expr) -> syn::Result<String> {
    if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = expr {
        Ok(s.value())
    } else {
        Err(Error::new_spanned(expr, "invalid from expression"))
    }
}
