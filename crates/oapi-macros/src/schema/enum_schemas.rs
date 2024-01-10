use std::borrow::Cow;

use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::{quote, ToTokens};
use syn::{punctuated::Punctuated, Attribute, Fields, Token, Variant};

use crate::{
    doc_comment::CommentAttributes,
    feature::{
        parse_features, pop_feature, pop_feature_as_inner, Example, Feature, FeaturesExt, IntoInner, Rename, RenameAll,
        Symbol, ToTokensExt,
    },
    schema::{Inline, VariantRename},
    serde_util::{self, SerdeContainer, SerdeEnumRepr, SerdeValue},
    type_tree::{TypeTree, ValueType},
};

use super::{
    enum_variant::{
        self, AdjacentlyTaggedEnum, CustomEnum, Enum, ObjectVariant, SimpleEnumVariant, TaggedEnum, UntaggedEnum,
    },
    feature::{
        self, ComplexEnumFeatures, EnumFeatures, EnumNamedFieldVariantFeatures, EnumUnnamedFieldVariantFeatures,
        FromAttributes,
    },
    is_not_skipped, NamedStructSchema, SchemaFeatureExt, UnnamedStructSchema,
};

#[derive(Debug)]
pub(crate) struct EnumSchema<'a> {
    pub(super) schema_type: EnumSchemaType<'a>,
    pub(super) symbol: Option<Symbol>,
    pub(super) inline: Option<Inline>,
}

impl<'e> EnumSchema<'e> {
    pub(crate) fn new(
        enum_name: Cow<'e, str>,
        variants: &'e Punctuated<Variant, Token![,]>,
        attributes: &'e [Attribute],
    ) -> Self {
        if variants.iter().all(|variant| matches!(variant.fields, Fields::Unit)) {
            #[cfg(feature = "repr")]
            {
                attributes
                    .iter()
                    .find_map(|attribute| {
                        if attribute.path().is_ident("repr") {
                            attribute.parse_args::<syn::TypePath>().ok()
                        } else {
                            None
                        }
                    })
                    .map(|enum_type| {
                        let mut repr_enum_features = feature::parse_schema_features_with(attributes, |input| {
                            Ok(parse_features!(
                                input as crate::feature::Example,
                                crate::feature::Default,
                                crate::feature::Symbol,
                                crate::feature::Inline
                            ))
                        })
                        .unwrap_or_default();

                        let symbol = pop_feature_as_inner!(repr_enum_features => Feature::Symbol(_v));
                        let inline: Option<Inline> = pop_feature_as_inner!(repr_enum_features => Feature::Inline(_v));
                        Self {
                            schema_type: EnumSchemaType::Repr(ReprEnum {
                                variants,
                                attributes,
                                enum_type,
                                enum_features: repr_enum_features,
                            }),
                            symbol,
                            inline,
                        }
                    })
                    .unwrap_or_else(|| {
                        let mut simple_enum_features = attributes
                            .parse_features::<EnumFeatures>()
                            .into_inner()
                            .unwrap_or_default();
                        let rename_all = simple_enum_features.pop_rename_all_feature();
                        let symbol = pop_feature_as_inner!(simple_enum_features => Feature::Symbol(_v));
                        let inline: Option<Inline> = pop_feature_as_inner!(simple_enum_features => Feature::Inline(_v));

                        Self {
                            schema_type: EnumSchemaType::Simple(SimpleEnum {
                                attributes,
                                variants,
                                enum_features: simple_enum_features,
                                rename_all,
                            }),
                            symbol,
                            inline,
                        }
                    })
            }

            #[cfg(not(feature = "repr"))]
            {
                let mut simple_enum_features = attributes
                    .parse_features::<EnumFeatures>()
                    .into_inner()
                    .unwrap_or_default();
                let rename_all = simple_enum_features.pop_rename_all_feature();
                let symbol: Option<Symbol> = pop_feature_as_inner!(simple_enum_features => Feature::Symbol(_v));
                let inline: Option<Inline> = pop_feature_as_inner!(simple_enum_features => Feature::Inline(_v));

                Self {
                    schema_type: EnumSchemaType::Simple(SimpleEnum {
                        attributes,
                        variants,
                        enum_features: simple_enum_features,
                        rename_all,
                    }),
                    symbol,
                    inline,
                }
            }
        } else {
            let mut enum_features = attributes
                .parse_features::<ComplexEnumFeatures>()
                .into_inner()
                .unwrap_or_default();
            let rename_all = enum_features.pop_rename_all_feature();
            let symbol: Option<Symbol> = pop_feature_as_inner!(enum_features => Feature::Symbol(_v));
            let inline: Option<Inline> = pop_feature_as_inner!(enum_features => Feature::Inline(_v));

            Self {
                schema_type: EnumSchemaType::Complex(ComplexEnum {
                    enum_name,
                    attributes,
                    variants,
                    rename_all,
                    enum_features,
                }),
                symbol,
                inline,
            }
        }
    }
}

impl ToTokens for EnumSchema<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.schema_type.to_tokens(tokens);
    }
}

#[derive(Debug)]
pub(super) enum EnumSchemaType<'e> {
    Simple(SimpleEnum<'e>),
    #[cfg(feature = "repr")]
    Repr(ReprEnum<'e>),
    Complex(ComplexEnum<'e>),
}

impl ToTokens for EnumSchemaType<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let attributes = match self {
            Self::Simple(simple) => {
                simple.to_tokens(tokens);
                simple.attributes
            }
            #[cfg(feature = "repr")]
            Self::Repr(repr) => {
                repr.to_tokens(tokens);
                repr.attributes
            }
            Self::Complex(complex) => {
                complex.to_tokens(tokens);
                complex.attributes
            }
        };

        if let Some(deprecated) = crate::get_deprecated(attributes) {
            tokens.extend(quote! { .deprecated(#deprecated) });
        }

        let description = CommentAttributes::from_attributes(attributes).as_formatted_string();
        if !description.is_empty() {
            tokens.extend(quote! {
                .description(#description)
            })
        }
    }
}

#[cfg(feature = "repr")]
#[derive(Debug)]
pub(super) struct ReprEnum<'a> {
    variants: &'a Punctuated<Variant, Token![,]>,
    attributes: &'a [Attribute],
    enum_type: syn::TypePath,
    enum_features: Vec<Feature>,
}

#[cfg(feature = "repr")]
impl ToTokens for ReprEnum<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let container_rules = serde_util::parse_container(self.attributes);

        regular_enum_to_tokens(tokens, &container_rules, &self.enum_features, || {
            self.variants
                .iter()
                .filter_map(|variant| {
                    let variant_type = &variant.ident;
                    let variant_rules = serde_util::parse_value(&variant.attrs);

                    if is_not_skipped(&variant_rules) {
                        let repr_type = &self.enum_type;
                        Some(enum_variant::ReprVariant {
                            value: quote! { Self::#variant_type as #repr_type },
                            type_path: repr_type,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<enum_variant::ReprVariant<TokenStream>>>()
        });
    }
}

fn rename_enum_variant<'a>(
    name: &'a str,
    features: &mut Vec<Feature>,
    variant_rules: &'a Option<SerdeValue>,
    container_rules: &'a Option<SerdeContainer>,
    rename_all: &'a Option<RenameAll>,
) -> Option<Cow<'a, str>> {
    let rename = features.pop_rename_feature().map(|rename| rename.into_value());
    let rename_to = variant_rules
        .as_ref()
        .and_then(|variant_rules| variant_rules.rename.as_deref().map(Cow::Borrowed))
        .or_else(|| rename.map(Cow::Owned));

    let rename_all = container_rules
        .as_ref()
        .and_then(|container_rules| container_rules.rename_all.as_ref())
        .or_else(|| rename_all.as_ref().map(|rename_all| rename_all.as_rename_rule()));

    crate::rename::<VariantRename>(name, rename_to, rename_all)
}

#[derive(Debug)]
pub(super) struct SimpleEnum<'a> {
    variants: &'a Punctuated<Variant, Token![,]>,
    attributes: &'a [Attribute],
    enum_features: Vec<Feature>,
    rename_all: Option<RenameAll>,
}

impl ToTokens for SimpleEnum<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let container_rules = serde_util::parse_container(self.attributes);

        regular_enum_to_tokens(tokens, &container_rules, &self.enum_features, || {
            self.variants
                .iter()
                .filter_map(|variant| {
                    let variant_rules = serde_util::parse_value(&variant.attrs);

                    if is_not_skipped(&variant_rules) {
                        Some((variant, variant_rules))
                    } else {
                        None
                    }
                })
                .flat_map(|(variant, variant_rules)| {
                    let name = &*variant.ident.to_string();
                    let mut variant_features = feature::parse_schema_features_with(&variant.attrs, |input| {
                        Ok(parse_features!(input as Rename))
                    })
                    .unwrap_or_default();
                    let variant_name = rename_enum_variant(
                        name,
                        &mut variant_features,
                        &variant_rules,
                        &container_rules,
                        &self.rename_all,
                    );

                    variant_name
                        .map(|name| SimpleEnumVariant {
                            value: name.to_token_stream(),
                        })
                        .or_else(|| {
                            Some(SimpleEnumVariant {
                                value: name.to_token_stream(),
                            })
                        })
                })
                .collect::<Vec<SimpleEnumVariant<TokenStream>>>()
        });
    }
}

fn regular_enum_to_tokens<T: self::enum_variant::Variant>(
    tokens: &mut TokenStream,
    container_rules: &Option<SerdeContainer>,
    enum_variant_features: &Vec<Feature>,
    get_variants_tokens_vec: impl FnOnce() -> Vec<T>,
) {
    let enum_values = get_variants_tokens_vec();

    tokens.extend(match container_rules {
        Some(serde_container) => match &serde_container.enum_repr {
            SerdeEnumRepr::ExternallyTagged => Enum::new(enum_values).to_token_stream(),
            SerdeEnumRepr::InternallyTagged { tag } => TaggedEnum::new(
                enum_values
                    .into_iter()
                    .map(|variant| (Cow::Borrowed(tag.as_str()), variant)),
            )
            .to_token_stream(),
            SerdeEnumRepr::Untagged => UntaggedEnum::new().to_token_stream(),
            SerdeEnumRepr::AdjacentlyTagged { tag, content } => AdjacentlyTaggedEnum::new(
                enum_values
                    .into_iter()
                    .map(|variant| (Cow::Borrowed(tag.as_str()), Cow::Borrowed(content.as_str()), variant)),
            )
            .to_token_stream(),
            // This should not be possible as serde should not let that happen
            SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => panic!("Invalid serde enum repr"),
        },
        _ => Enum::new(enum_values).to_token_stream(),
    });

    tokens.extend(enum_variant_features.to_token_stream());
}

#[derive(Debug)]
pub(super) struct ComplexEnum<'a> {
    variants: &'a Punctuated<Variant, Token![,]>,
    attributes: &'a [Attribute],
    enum_name: Cow<'a, str>,
    enum_features: Vec<Feature>,
    rename_all: Option<RenameAll>,
}

impl ComplexEnum<'_> {
    /// Produce tokens that represent a variant of a [`ComplexEnum`].
    fn variant_tokens(
        &self,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> TokenStream {
        // TODO need to be able to split variant.attrs for variant and the struct representation!
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (symbol_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_symbol())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(named_struct_features => Feature::Example(_));

                self::enum_variant::Variant::to_tokens(&ObjectVariant {
                    name: variant_name.unwrap_or(Cow::Borrowed(&name)),
                    symbol: symbol_features.first().map(ToTokens::to_token_stream),
                    example: example.as_ref().map(ToTokens::to_token_stream),
                    item: NamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        rename_all: named_struct_features.pop_rename_all_feature(),
                        features: Some(named_struct_features),
                        fields: &named_fields.named,
                        generics: None,
                        symbol: None,
                        inline: None,
                    },
                })
            }
            Fields::Unnamed(unnamed_fields) => {
                let (symbol_features, mut unnamed_struct_features) = variant
                    .attrs
                    .parse_features::<EnumUnnamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_symbol())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unnamed_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(unnamed_struct_features => Feature::Example(_));

                self::enum_variant::Variant::to_tokens(&ObjectVariant {
                    name: variant_name.unwrap_or(Cow::Borrowed(&name)),
                    symbol: symbol_features.first().map(ToTokens::to_token_stream),
                    example: example.as_ref().map(ToTokens::to_token_stream),
                    item: UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        symbol: None,
                        inline: None,
                    },
                })
            }
            Fields::Unit => {
                let mut unit_features = feature::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(
                        input as crate::feature::Symbol,
                        RenameAll,
                        Rename,
                        Example
                    ))
                })
                .unwrap_or_default();
                let symbol = pop_feature!(unit_features => Feature::Symbol(_));
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(unit_features => Feature::Example(_));
                let description = CommentAttributes::from_attributes(&variant.attrs).as_formatted_string();
                let description = (!description.is_empty()).then(|| Feature::Description(description.into()));

                // Unit variant is just simple enum with single variant.
                let mut sev = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);
                if let Some(symbol) = symbol {
                    sev = sev.symbol(symbol.to_token_stream());
                }
                if let Some(example) = example {
                    sev = sev.example(example.to_token_stream());
                }
                if let Some(description) = description {
                    sev = sev.description(description.to_token_stream());
                }
                sev.to_token_stream()
            }
        }
    }

    /// Produce tokens that represent a variant of a [`ComplexEnum`] where serde enum attribute
    /// `untagged` applies.
    fn untagged_variant_tokens(&self, variant: &Variant) -> TokenStream {
        match &variant.fields {
            Fields::Named(named_fields) => {
                let mut named_struct_features = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .unwrap_or_default();

                NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    symbol: None,
                    inline: None,
                }
                .to_token_stream()
            }
            Fields::Unnamed(unnamed_fields) => {
                let unnamed_struct_features = variant
                    .attrs
                    .parse_features::<EnumUnnamedFieldVariantFeatures>()
                    .into_inner()
                    .unwrap_or_default();

                UnnamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    features: Some(unnamed_struct_features),
                    fields: &unnamed_fields.unnamed,
                    symbol: None,
                    inline: None,
                }
                .to_token_stream()
            }
            Fields::Unit => {
                let mut unit_features = feature::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(input as crate::feature::Symbol))
                })
                .unwrap_or_default();
                let symbol = pop_feature!(unit_features => Feature::Symbol(_));

                UntaggedEnum::with_symbol(symbol).to_token_stream()
            }
        }
    }

    /// Produce tokens that represent a variant of a [`ComplexEnum`] where serde enum attribute
    /// `tag = ` applies.
    fn tagged_variant_tokens(
        &self,
        tag: &str,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> TokenStream {
        let oapi = crate::oapi_crate();
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (symbol_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_symbol())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let named_enum = NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    symbol: None,
                    inline: None,
                };
                let symbol = symbol_features.first().map(ToTokens::to_token_stream);

                let variant_name_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);
                quote! {
                    #named_enum
                        #symbol
                        .property(#tag, #variant_name_tokens)
                        .required(#tag)
                }
            }
            Fields::Unnamed(unnamed_fields) => {
                if unnamed_fields.unnamed.len() == 1 {
                    let (symbol_features, mut unnamed_struct_features) = variant
                        .attrs
                        .parse_features::<EnumUnnamedFieldVariantFeatures>()
                        .into_inner()
                        .map(|features| features.split_for_symbol())
                        .unwrap_or_default();
                    let variant_name = rename_enum_variant(
                        name.as_ref(),
                        &mut unnamed_struct_features,
                        variant_rules,
                        container_rules,
                        rename_all,
                    );

                    let unnamed_enum = UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        symbol: None,
                        inline: None,
                    };

                    let symbol = symbol_features.first().map(ToTokens::to_token_stream);
                    let variant_name_tokens = Enum::new([SimpleEnumVariant {
                        value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                    }]);

                    let is_reference = unnamed_fields.unnamed.iter().any(|field| {
                        let ty = TypeTree::from_type(&field.ty);

                        ty.value_type == ValueType::Object
                    });

                    if is_reference {
                        quote! {
                            #oapi::oapi::schema::AllOf::new()
                                #symbol
                                .item(#unnamed_enum)
                                .item(#oapi::oapi::schema::Object::new()
                                    .schema_type(#oapi::oapi::schema::SchemaType::Object)
                                    .property(#tag, #variant_name_tokens)
                                    .required(#tag)
                                )
                        }
                    } else {
                        quote! {
                            #unnamed_enum
                                #symbol
                                .schema_type(#oapi::oapi::schema::SchemaType::Object)
                                .property(#tag, #variant_name_tokens)
                                .required(#tag)
                        }
                    }
                } else {
                    abort!(
                        variant,
                        "Unnamed (tuple) enum variants are unsupported for internally tagged enums using the `tag = ` serde attribute";

                        help = "Try using a different serde enum representation";
                        note = "See more about enum limitations here: `https://serde.rs/enum-representations.html#internally-tagged`"
                    );
                }
            }
            Fields::Unit => {
                let mut unit_features = feature::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(input as crate::feature::Symbol, Rename))
                })
                .unwrap_or_default();
                let symbol = pop_feature!(unit_features => Feature::Symbol(_));

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                // Unit variant is just simple enum with single variant.
                let variant_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);

                quote! {
                    #oapi::oapi::schema::Object::new()
                        #symbol
                        .property(#tag, #variant_tokens)
                        .required(#tag)
                }
            }
        }
    }

    // FIXME perhaps design this better to lessen the amount of args.
    #[allow(clippy::too_many_arguments)]
    fn adjacently_tagged_variant_tokens(
        &self,
        tag: &str,
        content: &str,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> TokenStream {
        let oapi = crate::oapi_crate();
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (symbol_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_symbol())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let named_enum = NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    symbol: None,
                    inline: None,
                };
                let symbol = symbol_features.first().map(ToTokens::to_token_stream);

                let variant_name_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);
                quote! {
                    #oapi::oapi::schema::Object::new()
                        #symbol
                        .schema_type(#oapi::oapi::schema::SchemaType::Object)
                        .property(#tag, #variant_name_tokens)
                        .required(#tag)
                        .property(#content, #named_enum)
                        .required(#content)
                }
            }
            Fields::Unnamed(unnamed_fields) => {
                if unnamed_fields.unnamed.len() == 1 {
                    let (symbol_features, mut unnamed_struct_features) = variant
                        .attrs
                        .parse_features::<EnumUnnamedFieldVariantFeatures>()
                        .into_inner()
                        .map(|features| features.split_for_symbol())
                        .unwrap_or_default();
                    let variant_name = rename_enum_variant(
                        name.as_ref(),
                        &mut unnamed_struct_features,
                        variant_rules,
                        container_rules,
                        rename_all,
                    );

                    let unnamed_enum = UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        symbol: None,
                        inline: None,
                    };

                    let symbol = symbol_features.first().map(ToTokens::to_token_stream);
                    let variant_name_tokens = Enum::new([SimpleEnumVariant {
                        value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                    }]);

                    quote! {
                        #oapi::oapi::schema::Object::new()
                            #symbol
                            .schema_type(#oapi::oapi::schema::SchemaType::Object)
                            .property(#tag, #variant_name_tokens)
                            .required(#tag)
                            .property(#content, #unnamed_enum)
                            .required(#content)
                    }
                } else {
                    abort!(
                        variant,
                        "Unnamed (tuple) enum variants are unsupported for adjacently tagged enums using the `tag = <tag>, content = <content>` serde attribute";

                        help = "Try using a different serde enum representation";
                        note = "See more about enum limitations here: `https://serde.rs/enum-representations.html#adjacently-tagged`"
                    );
                }
            }
            Fields::Unit => {
                // In this case `content` is simply ignored - there is nothing to put in it.

                let mut unit_features = feature::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(input as crate::feature::Symbol, Rename))
                })
                .unwrap_or_default();
                let symbol = pop_feature!(unit_features => Feature::Symbol(_));

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                // Unit variant is just simple enum with single variant.
                let variant_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);

                quote! {
                    #oapi::oapi::schema::Object::new()
                        #symbol
                        .property(#tag, #variant_tokens)
                        .required(#tag)
                }
            }
        }
    }
}

impl ToTokens for ComplexEnum<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let attributes = &self.attributes;
        let container_rules = serde_util::parse_container(attributes);

        let enum_repr = container_rules
            .as_ref()
            .map(|rules| rules.enum_repr.clone())
            .unwrap_or_default();
        let tag = match &enum_repr {
            SerdeEnumRepr::AdjacentlyTagged { tag, .. } | SerdeEnumRepr::InternallyTagged { tag } => Some(tag),
            SerdeEnumRepr::ExternallyTagged
            | SerdeEnumRepr::Untagged
            | SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => None,
        };

        let ts = self
            .variants
            .iter()
            .filter_map(|variant: &Variant| {
                let variant_serde_rules = serde_util::parse_value(&variant.attrs);
                if is_not_skipped(&variant_serde_rules) {
                    Some((variant, variant_serde_rules))
                } else {
                    None
                }
            })
            .map(|(variant, variant_serde_rules)| {
                let variant_name = &*variant.ident.to_string();

                match &enum_repr {
                    SerdeEnumRepr::ExternallyTagged => self.variant_tokens(
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::InternallyTagged { tag } => self.tagged_variant_tokens(
                        tag,
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::Untagged => self.untagged_variant_tokens(variant),
                    SerdeEnumRepr::AdjacentlyTagged { tag, content } => self.adjacently_tagged_variant_tokens(
                        tag,
                        content,
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => {
                        unreachable!("Serde should not have parsed an UnfinishedAdjacentlyTagged")
                    }
                }
            })
            .collect::<CustomEnum<'_, TokenStream>>();
        if let Some(tag) = tag {
            ts.discriminator(Cow::Borrowed(tag.as_str())).to_tokens(tokens);
        } else {
            ts.to_tokens(tokens);
        }

        tokens.extend(self.enum_features.to_token_stream());
    }
}
