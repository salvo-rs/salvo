use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    DeriveInput, Error, Expr, ExprLit, ExprPath, Field, Generics, Lit, Meta, MetaNameValue, Token,
    Type,
};

use crate::{
    attribute, omit_type_path_lifetimes, salvo_crate,
    serde_util::{self, RenameRule, SerdeValue},
};

struct FieldInfo {
    ident: Option<Ident>,
    ty: Type,
    sources: Vec<SourceInfo>,
    aliases: Vec<String>,
    rename: Option<String>,
    serde_rename: Option<String>,
    flatten: bool,
}
impl TryFrom<&Field> for FieldInfo {
    type Error = Error;

    fn try_from(field: &Field) -> Result<Self, Self::Error> {
        let ident = field.ident.clone();
        let attrs = field.attrs.clone();
        let mut sources: Vec<SourceInfo> = Vec::with_capacity(field.attrs.len());
        let mut aliases = Vec::with_capacity(field.attrs.len());
        let mut rename = None;
        let mut flatten = None;
        for attr in attrs {
            if attr.path().is_ident("salvo") {
                if let Ok(Some(metas)) = attribute::find_nested_list(&attr, "extract") {
                    let info: ExtractFieldInfo = metas.parse_args()?;
                    sources.extend(info.sources);
                    aliases.extend(info.aliases);
                    flatten = info.flatten;
                    if info.rename.is_some() {
                        rename = info.rename;
                    }
                    if info.flatten.is_some() {
                        flatten = info.flatten;
                    }
                }
            }
        }
        sources.dedup();
        aliases.dedup();

        let (serde_rename, serde_flatten) = if let Some(SerdeValue {
            rename, flatten, ..
        }) = serde_util::parse_value(&field.attrs)
        {
            (rename, flatten)
        } else {
            (None, false)
        };
        let flatten = flatten.unwrap_or(serde_flatten);
        if flatten {
            if !sources.is_empty() {
                return Err(Error::new_spanned(
                    ident,
                    "flatten field should not define sources.",
                ));
            }
            if !aliases.is_empty() {
                return Err(Error::new_spanned(
                    ident,
                    "flatten field should not define aliases.",
                ));
            }
        }

        Ok(Self {
            ident,
            ty: field.ty.clone(),
            sources,
            aliases,
            rename,
            serde_rename,
            flatten,
        })
    }
}

#[derive(Default, Debug)]
struct ExtractFieldInfo {
    sources: Vec<SourceInfo>,
    aliases: Vec<String>,
    rename: Option<String>,
    flatten: Option<bool>,
}
impl Parse for ExtractFieldInfo {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut extract = Self::default();
        while !input.is_empty() {
            let id: String = input.parse::<syn::Ident>()?.to_string();
            match &*id {
                "source" => {
                    let item;
                    syn::parenthesized!(item in input);
                    extract.sources.push(item.parse::<SourceInfo>()?);
                }
                "rename" => {
                    input.parse::<Token![=]>()?;
                    let expr = input.parse::<Expr>()?;
                    extract.rename = Some(parse_path_or_lit_str(&expr)?);
                }
                "alias" => {
                    input.parse::<Token![=]>()?;
                    let expr = input.parse::<Expr>()?;
                    extract.aliases.push(parse_path_or_lit_str(&expr)?);
                }
                "flatten" => {
                    extract.flatten = Some(true);
                }
                _ => {
                    return Err(input.error("unexpected attribute"));
                }
            }
            let _ = input.parse::<Token![,]>();
        }
        Ok(extract)
    }
}

#[derive(Eq, PartialEq, Debug)]
struct SourceInfo {
    from: String,
    parser: String,
}

impl Parse for SourceInfo {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut source = SourceInfo {
            from: "body".to_owned(),
            parser: "smart".to_owned(),
        };
        let fields: Punctuated<MetaNameValue, Token![,]> = Punctuated::parse_terminated(input)?;
        for field in fields {
            if field.path.is_ident("from") {
                source.from = parse_path_or_lit_str(&field.value)?.to_lowercase();
            } else if field.path.is_ident("parse") {
                source.parser = parse_path_or_lit_str(&field.value)?.to_lowercase();
            } else {
                return Err(input.error("unexpected attribute"));
            }
        }
        if source.parser.is_empty() {
            source.parser = "smart".to_string();
        }
        if !["param", "query", "header", "body"].contains(&source.from.as_str()) {
            return Err(Error::new(
                input.span(),
                format!("source from is invalid: {}", source.from),
            ));
        }
        if !["multimap", "json", "smart"].contains(&source.parser.as_str()) {
            return Err(Error::new(
                input.span(),
                format!("source parser is invalid: {}", source.parser),
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
    rename_all: Option<RenameRule>,
    serde_rename_all: Option<RenameRule>,
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
                    let nested =
                        metas.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)?;
                    for meta in nested {
                        match meta {
                            Meta::List(meta) => {
                                if meta.path.is_ident("default_source") {
                                    default_sources.push(meta.parse_args()?);
                                }
                            }
                            Meta::NameValue(meta) => {
                                if meta.path.is_ident("rename_all") {
                                    rename_all = Some(
                                        parse_path_or_lit_str(&meta.value)?
                                            .parse::<RenameRule>()?,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        let serde_container = serde_util::parse_container(&attrs);
        let serde_rename_all = serde_container.and_then(|c| c.rename_all);
        Ok(Self {
            ident,
            generics,
            fields,
            default_sources,
            rename_all,
            serde_rename_all,
        })
    }
}

fn metadata_source(salvo: &Ident, source: &SourceInfo) -> TokenStream {
    let from = Ident::new(
        &RenameRule::PascalCase.apply_to_field(&source.from),
        Span::call_site(),
    );
    let parser = if source.parser.to_lowercase() == "multimap" {
        Ident::new("MultiMap", Span::call_site())
    } else {
        Ident::new(
            &RenameRule::PascalCase.apply_to_field(&source.parser),
            Span::call_site(),
        )
    };
    let from = quote! {
        #salvo::extract::metadata::SourceFrom::#from
    };
    let parser = quote! {
        #salvo::extract::metadata::SourceParser::#parser
    };
    quote! {
        #salvo::extract::metadata::Source::new(#from, #parser)
    }
}

pub(crate) fn generate(args: DeriveInput) -> Result<TokenStream, Error> {
    let mut args: ExtractibleArgs = ExtractibleArgs::from_derive_input(&args)?;
    let salvo = salvo_crate();
    let (_, ty_generics, where_clause) = args.generics.split_for_impl();

    let name = &args.ident;
    let mut default_sources = Vec::new();
    let mut fields = Vec::new();

    for source in &args.default_sources {
        let source = metadata_source(&salvo, source);
        default_sources.push(quote! {
            metadata = metadata.add_default_source(#source);
        });
    }
    fn quote_rename_rule(salvo: &Ident, rename_all: &RenameRule) -> TokenStream {
        let rename_all = match rename_all {
            RenameRule::LowerCase => "LowerCase",
            RenameRule::UpperCase => "UpperCase",
            RenameRule::PascalCase => "PascalCase",
            RenameRule::CamelCase => "CamelCase",
            RenameRule::SnakeCase => "SnakeCase",
            RenameRule::ScreamingSnakeCase => "ScreamingSnakeCase",
            RenameRule::KebabCase => "KebabCase",
            RenameRule::ScreamingKebabCase => "ScreamingKebabCase",
        };
        let rule = Ident::new(rename_all, Span::call_site());
        quote! {
            #salvo::extract::RenameRule::#rule
        }
    }
    let rename_all = if let Some(rename_all) = &args.rename_all {
        let rule = quote_rename_rule(&salvo, rename_all);
        Some(quote! {
            metadata = metadata.rename_all(#rule);
        })
    } else {
        None
    };
    let serde_rename_all = if let Some(serde_rename_all) = &args.serde_rename_all {
        let rule = quote_rename_rule(&salvo, serde_rename_all);
        Some(quote! {
            metadata = metadata.serde_rename_all(#rule);
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
        let mut nested_metadata = None;
        let mut sources = Vec::with_capacity(field.sources.len());
        if field.flatten {
            if let Type::Path(ty) = &field.ty {
                let ty = omit_type_path_lifetimes(ty);
                nested_metadata = Some(quote! {
                    field = field.metadata(<#ty as #salvo::extract::Extractible>::metadata());
                    field = field.flatten(true);
                });
            } else {
                return Err(Error::new_spanned(name, "Invalid type for request source."));
            }
        } else {
            for source in &field.sources {
                let source = metadata_source(&salvo, source);
                sources.push(quote! {
                    field = field.add_source(#source);
                });
            }
        }
        if nested_metadata.is_some() && field.sources.len() > 1 {
            return Err(Error::new_spanned(
                name,
                "Only one source can be from request.",
            ));
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
        let serde_rename = field.serde_rename.as_ref().map(|serde_rename| {
            quote! {
                field = field.serde_rename(#serde_rename);
            }
        });
        fields.push(quote! {
            let mut field = #salvo::extract::metadata::Field::new(#field_ident);
            #nested_metadata
            #(#sources)*
            #(#aliases)*
            #rename
            #serde_rename
            metadata = metadata.add_field(field);
        });
    }

    let mt = name.to_string();
    let metadata = quote! {
        fn metadata() ->  &'static #salvo::extract::Metadata {
            static METADATA: ::std::sync::OnceLock<#salvo::extract::Metadata> = ::std::sync::OnceLock::new();
            METADATA.get_or_init(|| {
                let mut metadata = #salvo::extract::Metadata::new(#mt);
                #(
                    #default_sources
                )*
                #rename_all
                #serde_rename_all
                #(
                    #fields
                )*
                metadata
            })
        }
    };
    let life_param = args.generics.lifetimes().next();
    let code = if let Some(life_param) = life_param {
        let ex_life_def = syn::parse_str(&format!("'__macro_gen_ex:{}", life_param.lifetime))
            .expect("Invalid lifetime.");
        let mut generics = args.generics.clone();
        generics.params.insert(0, ex_life_def);
        let impl_generics_de = generics.split_for_impl().0;
        quote! {
            impl #impl_generics_de #salvo::extract::Extractible<'__macro_gen_ex> for #name #ty_generics #where_clause {
                #metadata

                #[allow(refining_impl_trait)]
                async fn extract(req: &'__macro_gen_ex mut #salvo::http::Request) -> Result<Self, #salvo::http::ParseError>
                where
                    Self: Sized {
                    #salvo::serde::from_request(req, Self::metadata()).await
                }
            }
        }
    } else {
        let ex_life_def = syn::parse_str("'__macro_gen_ex").expect("Invalid lifetime.");
        let mut generics = args.generics.clone();
        generics.params.insert(0, ex_life_def);
        let impl_generics_de = generics.split_for_impl().0;
        quote! {
            impl #impl_generics_de #salvo::extract::Extractible<'__macro_gen_ex> for #name #ty_generics #where_clause {
                #metadata

                #[allow(refining_impl_trait)]
                async fn extract(req: &'__macro_gen_ex mut #salvo::http::Request) -> Result<Self, #salvo::http::ParseError>
                where
                    Self: Sized {
                    #salvo::serde::from_request(req, Self::metadata()).await
                }
            }
        }
    };
    Ok(code)
}

fn parse_path_or_lit_str(expr: &Expr) -> syn::Result<String> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Ok(s.value()),
        Expr::Path(ExprPath { path, .. }) => Ok(path.require_ident()?.to_string()),
        _ => Err(Error::new_spanned(expr, "invalid indent or lit str")),
    }
}
