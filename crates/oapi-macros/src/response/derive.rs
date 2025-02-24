use std::borrow::Cow;
use std::{iter, mem};

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, Field, Fields, Generics, LitStr, Meta, Path, Token, Type, TypePath, Variant,
};

use crate::doc_comment::CommentAttributes;
use crate::operation::{InlineType, PathType};
use crate::schema::{EnumSchema, NamedStructSchema};
use crate::{Array, DiagLevel, DiagResult, Diagnostic, TryToTokens, attribute, parse_utils};

use super::{
    Content, DeriveResponseValue, DeriveResponsesAttributes, DeriveToResponseValue,
    DeriveToResponsesValue, ResponseTuple, ResponseValue,
};

pub(crate) struct ToResponse<'a> {
    ident: &'a Ident,
    generics: &'a Generics,
    response: ResponseTuple<'a>,
}

impl<'a> ToResponse<'a> {
    pub(crate) fn new(
        attributes: &'a [Attribute],
        data: &'a Data,
        ident: &'a Ident,
        generics: &'a Generics,
    ) -> DiagResult<ToResponse<'a>> {
        let response = match &data {
            Data::Struct(struct_value) => match &struct_value.fields {
                Fields::Named(fields) => {
                    ToResponseNamedStructResponse::new(attributes, ident, &fields.named)?.0
                }
                Fields::Unnamed(fields) => {
                    let field = fields
                        .unnamed
                        .iter()
                        .next()
                        .expect("unnamed struct must have 1 field");
                    ToResponseUnnamedStructResponse::new(attributes, &field.ty, &field.attrs)?.0
                }
                Fields::Unit => ToResponseUnitStructResponse::new(attributes)?.0,
            },
            Data::Enum(enum_value) => EnumResponse::new(ident, &enum_value.variants, attributes)?.0,
            Data::Union(_) => {
                return Err(Diagnostic::spanned(
                    ident.span(),
                    DiagLevel::Error,
                    "`ToResponse` does not support `Union` type",
                ));
            }
        };

        Ok(Self {
            ident,
            generics,
            response,
        })
    }
}

impl TryToTokens for ToResponse<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let (_, ty_generics, where_clause) = self.generics.split_for_impl();

        let ident = &self.ident;
        let name = ident.to_string();
        let response = self.response.try_to_token_stream()?;

        let (impl_generics, _, _) = self.generics.split_for_impl();

        tokens.extend(quote! {
            impl #impl_generics #oapi::oapi::ToResponse for #ident #ty_generics #where_clause {
                fn to_response(components: &mut #oapi::oapi::Components) -> #oapi::oapi::RefOr<#oapi::oapi::Response> {
                    let response = #response;
                    components.responses.insert(#name, response);
                    #oapi::oapi::RefOr::Ref(#oapi::oapi::Ref::new(format!("#/components/responses/{}", #name)))
                }
            }
            impl #impl_generics #oapi::oapi::EndpointOutRegister for #ident #ty_generics #where_clause {
                fn register(components: &mut #oapi::oapi::Components, operation: &mut #oapi::oapi::Operation) {
                    operation.responses.insert("200", <Self as #oapi::oapi::ToResponse>::to_response(components))
                }
            }
        });
        Ok(())
    }
}

pub(crate) struct ToResponses<'a> {
    pub(crate) attributes: &'a [Attribute],
    pub(crate) data: &'a Data,
    pub(crate) generics: &'a Generics,
    pub(crate) ident: &'a Ident,
}

impl TryToTokens for ToResponses<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let responses = match &self.data {
            Data::Struct(struct_value) => match &struct_value.fields {
                Fields::Named(fields) => {
                    let response =
                        NamedStructResponse::new(self.attributes, self.ident, &fields.named)?.0;
                    let status_code = &response.status_code;
                    let response = response.try_to_token_stream()?;
                    Array::from_iter(iter::once(quote!((#status_code, #response))))
                }
                Fields::Unnamed(fields) => {
                    let field = fields
                        .unnamed
                        .iter()
                        .next()
                        .expect("Unnamed struct must have 1 field");

                    let response =
                        UnnamedStructResponse::new(self.attributes, &field.ty, &field.attrs)?.0;
                    let status_code = &response.status_code;
                    let response = response.try_to_token_stream()?;
                    Array::from_iter(iter::once(quote!((#status_code, #response))))
                }
                Fields::Unit => {
                    let response = UnitStructResponse::new(self.attributes)?.0;
                    let status_code = &response.status_code;
                    let response = response.try_to_token_stream()?;
                    Array::from_iter(iter::once(quote!((#status_code, #response))))
                }
            },
            Data::Enum(enum_value) => enum_value
                .variants
                .iter()
                .map(|variant| match &variant.fields {
                    Fields::Named(fields) => Ok(NamedStructResponse::new(
                        &variant.attrs,
                        &variant.ident,
                        &fields.named,
                    )?
                    .0),
                    Fields::Unnamed(fields) => {
                        let field = fields
                            .unnamed
                            .iter()
                            .next()
                            .expect("Unnamed enum variant must have 1 field");
                        Ok(UnnamedStructResponse::new(&variant.attrs, &field.ty, &field.attrs)?.0)
                    }
                    Fields::Unit => Ok(UnitStructResponse::new(&variant.attrs)?.0),
                })
                .collect::<Result<Vec<ResponseTuple>, Diagnostic>>()?
                .iter()
                .map(|response| {
                    let status_code = &response.status_code;
                    let response = response.try_to_token_stream()?;
                    Ok(quote!((#status_code, #oapi::oapi::RefOr::from(#response))))
                })
                .collect::<DiagResult<Array<TokenStream>>>()?,
            Data::Union(_) => {
                return Err(Diagnostic::spanned(
                    self.ident.span(),
                    DiagLevel::Error,
                    "`ToResponses` does not support `Union` type",
                ));
            }
        };

        let ident = &self.ident;
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        let responses = if !responses.is_empty() {
            quote!( #responses.into())
        } else {
            quote!( #oapi::oapi::Responses::new())
        };
        tokens.extend(quote! {
            impl #impl_generics #oapi::oapi::ToResponses for #ident #ty_generics #where_clause {
                fn to_responses(components: &mut #oapi::oapi::Components) -> #oapi::oapi::response::Responses {
                    #responses
                }
            }
            impl #impl_generics #oapi::oapi::EndpointOutRegister for #ident #ty_generics #where_clause {
                fn register(components: &mut #oapi::oapi::Components, operation: &mut #oapi::oapi::Operation) {
                    operation.responses.append(&mut <Self as #oapi::oapi::ToResponses>::to_responses(components));
                }
            }
        });
        Ok(())
    }
}

trait Response {
    fn to_type(ident: &Ident) -> Type {
        let path = Path::from(ident.clone());
        let type_path = TypePath { path, qself: None };
        Type::Path(type_path)
    }

    fn has_no_field_attributes(attr: &Attribute) -> (bool, &'static str) {
        const ERROR: &str =
            "Unexpected field attribute, field attributes are only supported at unnamed fields";

        if let Some(metas) = attribute::find_nested_list(attr, "response").ok().flatten() {
            if let Ok(metas) =
                metas.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            {
                for meta in metas {
                    if meta.path().is_ident("name") || meta.path().is_ident("content") {
                        return (false, ERROR);
                    }
                }
            }
        }
        (true, ERROR)
    }

    fn validate_attributes<'a, I: IntoIterator<Item = &'a Attribute>>(
        attributes: I,
        validate: impl Fn(&Attribute) -> (bool, &'static str),
    ) -> impl Iterator<Item = Diagnostic> {
        attributes.into_iter().filter_map(move |attribute| {
            let (valid, message) = validate(attribute);
            if !valid {
                Some(Diagnostic::spanned(
                    attribute.span(),
                    DiagLevel::Error,
                    message,
                ))
            } else {
                None
            }
        })
    }
}

struct UnnamedStructResponse<'u>(ResponseTuple<'u>);

impl Response for UnnamedStructResponse<'_> {}

impl<'u> UnnamedStructResponse<'u> {
    fn new(
        attributes: &[Attribute],
        ty: &'u Type,
        inner_attributes: &[Attribute],
    ) -> DiagResult<Self> {
        let mut is_inline = false;
        for attr in inner_attributes {
            if attr.path().is_ident("salvo")
                && attribute::has_nested_path(attr, "response", "inline").unwrap_or(false)
            {
                is_inline = true;
                break;
            }
        }
        let mut derive_value = DeriveToResponsesValue::from_attributes(attributes)?
            .expect("`ToResponses` must have `#[salvo(response(...))]` attribute");
        let description = {
            let s = CommentAttributes::from_attributes(attributes).as_formatted_string();
            parse_utils::LitStrOrExpr::LitStr(LitStr::new(&s, Span::call_site()))
        };
        let status_code = mem::take(&mut derive_value.status_code);
        Ok(Self(
            (
                status_code,
                ResponseValue::from_derive_to_responses_value(derive_value, description)
                    .response_type(PathType::MediaType(InlineType {
                        ty: Cow::Borrowed(ty),
                        is_inline,
                    })),
            )
                .into(),
        ))
    }
}

struct NamedStructResponse<'n>(ResponseTuple<'n>);

impl Response for NamedStructResponse<'_> {}

impl NamedStructResponse<'_> {
    fn new(
        attributes: &[Attribute],
        ident: &Ident,
        fields: &Punctuated<Field, Token![,]>,
    ) -> DiagResult<Self> {
        if let Some(diag) = Self::validate_attributes(attributes, Self::has_no_field_attributes)
            .chain(Self::validate_attributes(
                fields.iter().flat_map(|field| &field.attrs),
                Self::has_no_field_attributes,
            ))
            .next()
        {
            return Err(diag);
        }

        let mut derive_value = DeriveToResponsesValue::from_attributes(attributes)?
            .expect("`ToResponses` must have `#[salvo(response(...))]` attribute");
        let description = {
            let s = CommentAttributes::from_attributes(attributes).as_formatted_string();
            parse_utils::LitStrOrExpr::LitStr(LitStr::new(&s, Span::call_site()))
        };
        let status_code = mem::take(&mut derive_value.status_code);

        let inline_schema = NamedStructSchema {
            attributes,
            fields,
            aliases: None,
            description: None,
            features: None,
            generics: None,
            rename_all: None,
            struct_name: Cow::Owned(ident.to_string()),
            name: None,
            inline: None,
        };

        let ty = Self::to_type(ident);

        Ok(Self(
            (
                status_code,
                ResponseValue::from_derive_to_responses_value(derive_value, description)
                    .response_type(PathType::InlineSchema(
                        inline_schema.try_to_token_stream()?,
                        ty,
                    )),
            )
                .into(),
        ))
    }
}

struct UnitStructResponse<'u>(ResponseTuple<'u>);

impl Response for UnitStructResponse<'_> {}

impl UnitStructResponse<'_> {
    fn new(attributes: &[Attribute]) -> DiagResult<Self> {
        if let Some(diagnostics) =
            Self::validate_attributes(attributes, Self::has_no_field_attributes).next()
        {
            return Err(diagnostics);
        }

        let mut derive_value = DeriveToResponsesValue::from_attributes(attributes)?
            .expect("`ToResponses` must have `#[salvo(response(...))]` attribute");
        let status_code = mem::take(&mut derive_value.status_code);
        let description = {
            let s = CommentAttributes::from_attributes(attributes).as_formatted_string();
            parse_utils::LitStrOrExpr::LitStr(LitStr::new(&s, Span::call_site()))
        };

        Ok(Self(
            (
                status_code,
                ResponseValue::from_derive_to_responses_value(derive_value, description),
            )
                .into(),
        ))
    }
}

struct ToResponseNamedStructResponse<'p>(ResponseTuple<'p>);

impl Response for ToResponseNamedStructResponse<'_> {}

impl ToResponseNamedStructResponse<'_> {
    fn new(
        attributes: &[Attribute],
        ident: &Ident,
        fields: &Punctuated<Field, Token![,]>,
    ) -> DiagResult<Self> {
        if let Some(diag) = Self::validate_attributes(attributes, Self::has_no_field_attributes)
            .chain(Self::validate_attributes(
                fields.iter().flat_map(|field| &field.attrs),
                Self::has_no_field_attributes,
            ))
            .next()
        {
            return Err(diag);
        }

        let derive_value = DeriveToResponseValue::from_attributes(attributes)?;
        let description = {
            let s = CommentAttributes::from_attributes(attributes).as_formatted_string();
            parse_utils::LitStrOrExpr::LitStr(LitStr::new(&s, Span::call_site()))
        };
        let ty = Self::to_type(ident);

        let inline_schema = NamedStructSchema {
            fields,
            aliases: None,
            description: None,
            features: None,
            generics: None,
            attributes,
            struct_name: Cow::Owned(ident.to_string()),
            rename_all: None,
            name: None,
            inline: None,
        };
        let response_type = PathType::InlineSchema(inline_schema.try_to_token_stream()?, ty);

        let mut response_value: ResponseValue = ResponseValue::from(DeriveResponsesAttributes {
            derive_value,
            description,
        });
        response_value.response_type = Some(response_type);

        Ok(Self(response_value.into()))
    }
}

struct ToResponseUnnamedStructResponse<'c>(ResponseTuple<'c>);

impl Response for ToResponseUnnamedStructResponse<'_> {}

impl<'u> ToResponseUnnamedStructResponse<'u> {
    fn new(
        attributes: &[Attribute],
        ty: &'u Type,
        inner_attributes: &[Attribute],
    ) -> DiagResult<Self> {
        if let Some(diag) = Self::validate_attributes(attributes, Self::has_no_field_attributes)
            .chain(Self::validate_attributes(inner_attributes, |attribute| {
                const ERROR: &str = "Unexpected attribute, `content` is only supported on unnamed field enum variant";
                if attribute.path().is_ident("content") {
                    (false, ERROR)
                } else {
                    (true, ERROR)
                }
            }))
            .next()
        {
            return Err(diag);
        }

        let derive_value = DeriveToResponseValue::from_attributes(attributes)?;
        let description = {
            let s = CommentAttributes::from_attributes(attributes).as_formatted_string();
            parse_utils::LitStrOrExpr::LitStr(LitStr::new(&s, Span::call_site()))
        };

        let mut is_inline = false;
        for attr in inner_attributes {
            if attr.path().is_ident("salvo")
                && attribute::has_nested_path(attr, "schema", "inline").unwrap_or(false)
            {
                is_inline = true;
                break;
            }
        }
        let mut response_value: ResponseValue = ResponseValue::from(DeriveResponsesAttributes {
            description,
            derive_value,
        });

        response_value.response_type = Some(PathType::MediaType(InlineType {
            ty: Cow::Borrowed(ty),
            is_inline,
        }));

        Ok(Self(response_value.into()))
    }
}

struct VariantAttributes<'r> {
    type_and_content: Option<(&'r Type, String)>,
    derive_value: Option<DeriveToResponseValue>,
    is_inline: bool,
}

struct EnumResponse<'r>(ResponseTuple<'r>);

impl Response for EnumResponse<'_> {}

impl<'r> EnumResponse<'r> {
    fn new(
        ident: &Ident,
        variants: &'r Punctuated<Variant, Token![,]>,
        attributes: &[Attribute],
    ) -> DiagResult<Self> {
        if let Some(diag) = Self::validate_attributes(attributes, Self::has_no_field_attributes)
            .chain(Self::validate_attributes(
                variants.iter().flat_map(|variant| &variant.attrs),
                Self::has_no_field_attributes,
            ))
            .next()
        {
            return Err(diag);
        }

        let ty = Self::to_type(ident);
        let description = {
            let s = CommentAttributes::from_attributes(attributes).as_formatted_string();
            parse_utils::LitStrOrExpr::LitStr(LitStr::new(&s, Span::call_site()))
        };

        let variants_content = variants
            .into_iter()
            .map(Self::parse_variant_attributes)
            .collect::<DiagResult<Vec<VariantAttributes>>>()?
            .into_iter()
            .filter_map(Self::to_content);
        let contents: Punctuated<Content, Token![,]> = Punctuated::from_iter(variants_content);

        let derive_value = DeriveToResponseValue::from_attributes(attributes)?;
        if let Some(derive_value) = &derive_value {
            if (!contents.is_empty() && derive_value.example.is_some())
                || (!contents.is_empty() && derive_value.examples.is_some())
            {
                let ident = derive_value
                    .example
                    .as_ref()
                    .map(|(_, ident)| ident)
                    .or_else(|| derive_value.examples.as_ref().map(|(_, ident)| ident))
                    .expect("Expected `example` or `examples` to be present");
                return Err(Diagnostic::spanned(
                    ident.span(), DiagLevel::Error,
                    "Enum with `#[content]` attribute in variant cannot have enum level `example` or `examples` defined"
             ).help(format!("Try defining `{}` on the enum variant", ident)));
            }
        }

        let mut response_value: ResponseValue = From::from(DeriveResponsesAttributes {
            derive_value,
            description,
        });
        response_value.response_type = if contents.is_empty() {
            let inline_schema = EnumSchema::new(
                Cow::Owned(ident.to_string()),
                variants,
                attributes,
                None,
                None,
            )?;

            Some(PathType::InlineSchema(
                inline_schema.try_to_token_stream()?,
                ty,
            ))
        } else {
            None
        };
        response_value.contents = contents;

        Ok(Self(response_value.into()))
    }

    fn parse_variant_attributes(variant: &Variant) -> DiagResult<VariantAttributes> {
        let variant_derive_response_value =
            DeriveToResponseValue::from_attributes(variant.attrs.as_slice())?;
        // named enum variant should not have field attributes
        if let Fields::Named(named_fields) = &variant.fields {
            if let Some(diagnostic) = Self::validate_attributes(
                named_fields.named.iter().flat_map(|field| &field.attrs),
                Self::has_no_field_attributes,
            )
            .next()
            {
                return Err(diagnostic);
            }
        };

        let field = variant.fields.iter().next();

        let mut content_type = None;
        if let Some(attrs) = field.map(|f| &f.attrs) {
            for attr in attrs {
                if attr.path().is_ident("salvo") {
                    if let Some(metas) = attribute::find_nested_list(attr, "content").ok().flatten()
                    {
                        content_type = Some(
                            metas
                                .parse_args_with(|input: ParseStream| input.parse::<LitStr>())
                                .map_err(Diagnostic::from)?
                                .value(),
                        );
                        break;
                    }
                }
            }
        }

        let mut is_inline = false;
        if let Some(field) = field {
            for attr in &field.attrs {
                if attr.path().is_ident("salvo")
                    && attribute::has_nested_path(attr, "schema", "inline").unwrap_or(false)
                {
                    is_inline = true;
                    break;
                }
            }
        }

        Ok(VariantAttributes {
            type_and_content: field.map(|field| &field.ty).zip(content_type),
            derive_value: variant_derive_response_value,
            is_inline,
        })
    }

    fn to_content(
        VariantAttributes {
            type_and_content: field_and_content,
            mut derive_value,
            is_inline,
        }: VariantAttributes,
    ) -> Option<Content<'_>> {
        let (example, examples) = if let Some(variant_derive) = &mut derive_value {
            (
                mem::take(&mut variant_derive.example),
                mem::take(&mut variant_derive.examples),
            )
        } else {
            (None, None)
        };

        field_and_content.map(|(ty, content_type)| {
            Content(
                content_type,
                PathType::MediaType(InlineType {
                    ty: Cow::Borrowed(ty),
                    is_inline,
                }),
                example.map(|(example, _)| example),
                examples.map(|(examples, _)| examples),
            )
        })
    }
}

struct ToResponseUnitStructResponse<'u>(ResponseTuple<'u>);

impl Response for ToResponseUnitStructResponse<'_> {}

impl ToResponseUnitStructResponse<'_> {
    fn new(attributes: &[Attribute]) -> DiagResult<Self> {
        if let Some(diag) =
            Self::validate_attributes(attributes, Self::has_no_field_attributes).next()
        {
            return Err(diag);
        }

        let derive_value = DeriveToResponseValue::from_attributes(attributes)?;
        let description = {
            let s = CommentAttributes::from_attributes(attributes).as_formatted_string();
            parse_utils::LitStrOrExpr::LitStr(LitStr::new(&s, Span::call_site()))
        };
        let response_value: ResponseValue = ResponseValue::from(DeriveResponsesAttributes {
            derive_value,
            description,
        });

        Ok(Self(response_value.into()))
    }
}
