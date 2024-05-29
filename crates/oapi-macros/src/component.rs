use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;

use crate::doc_comment::CommentAttributes;
use crate::feature::{
    pop_feature, AdditionalProperties, Description, Feature, FeaturesExt, IsInline, Minimum, Nullable, TryToTokensExt,
    Validatable,
};
use crate::schema_type::{SchemaFormat, SchemaType};
use crate::type_tree::{GenericType, TypeTree, ValueType};
use crate::{Deprecated, DiagResult, Diagnostic, TryToTokens};

#[derive(Debug)]
pub(crate) struct ComponentSchemaProps<'c> {
    pub(crate) type_tree: &'c TypeTree<'c>,
    pub(crate) features: Option<Vec<Feature>>,
    pub(crate) description: Option<&'c ComponentDescription<'c>>,
    pub(crate) deprecated: Option<&'c Deprecated>,
    pub(crate) object_name: &'c str,
}

#[derive(Debug)]
pub(crate) enum ComponentDescription<'c> {
    CommentAttributes(&'c CommentAttributes),
    Description(&'c Description),
}

impl ToTokens for ComponentDescription<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let description = match self {
            Self::CommentAttributes(attributes) => {
                if attributes.is_empty() {
                    TokenStream::new()
                } else {
                    attributes.as_formatted_string().to_token_stream()
                }
            }
            Self::Description(description) => description.to_token_stream(),
        };

        if !description.is_empty() {
            tokens.extend(quote! {
                .description(#description)
            });
        }
    }
}

#[derive(Debug)]
pub(crate) struct ComponentSchema {
    tokens: TokenStream,
}

impl<'c> ComponentSchema {
    pub(crate) fn new(
        ComponentSchemaProps {
            type_tree,
            features,
            description,
            deprecated,
            object_name,
        }: ComponentSchemaProps,
    ) -> DiagResult<Self> {
        let mut tokens = TokenStream::new();
        let mut features = features.unwrap_or(Vec::new());
        let deprecated_stream = ComponentSchema::get_deprecated(deprecated);

        match type_tree.generic_type {
            Some(GenericType::Map) => {
                features.push(AdditionalProperties(true).into());
                ComponentSchema::map_to_tokens(
                    &mut tokens,
                    features,
                    type_tree,
                    object_name,
                    description,
                    deprecated_stream,
                )?
            }
            Some(GenericType::Vec) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
            )?,
            Some(GenericType::LinkedList) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
            )?,
            Some(GenericType::Set) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
            )?,
            #[cfg(feature = "smallvec")]
            Some(GenericType::SmallVec) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
            )?,
            Some(GenericType::Option) => {
                // Add nullable feature if not already exists. Option is always nullable
                if !features.iter().any(|feature| matches!(feature, Feature::Nullable(_))) {
                    features.push(Nullable::new().into());
                }

                ComponentSchema::new(ComponentSchemaProps {
                    type_tree: type_tree
                        .children
                        .as_ref()
                        .expect("ComponentSchema generic container type should have children")
                        .iter()
                        .next()
                        .expect("ComponentSchema generic container type should have 1 child"),
                    features: Some(features),
                    description,
                    deprecated,
                    object_name,
                })?
                .to_tokens(&mut tokens);
            }
            Some(GenericType::Cow)
            | Some(GenericType::Box)
            | Some(GenericType::Arc)
            | Some(GenericType::Rc)
            | Some(GenericType::RefCell) => {
                ComponentSchema::new(ComponentSchemaProps {
                    type_tree: type_tree
                        .children
                        .as_ref()
                        .expect("ComponentSchema generic container type should have children")
                        .iter()
                        .next()
                        .expect("ComponentSchema generic container type should have 1 child"),
                    features: Some(features),
                    description,
                    deprecated,
                    object_name,
                })?
                .to_tokens(&mut tokens);
            }
            None => ComponentSchema::non_generic_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
            )?,
        }

        Ok(Self { tokens })
    }

    fn map_to_tokens(
        tokens: &mut TokenStream,
        mut features: Vec<Feature>,
        type_tree: &TypeTree,
        object_name: &str,
        description_stream: Option<&ComponentDescription<'_>>,
        deprecated_stream: Option<TokenStream>,
    ) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let example = features.pop_by(|feature| matches!(feature, Feature::Example(_)));
        let additional_properties = pop_feature!(features => Feature::AdditionalProperties(_));
        let nullable = pop_feature!(features => Feature::Nullable(_));
        let default = pop_feature!(features => Feature::Default(_))
            .map(|f| f.try_to_token_stream())
            .transpose()?;

        let additional_properties = additional_properties
            .as_ref()
            .map(TryToTokens::try_to_token_stream)
            .transpose()
            .or_else(|_| {
                // Maps are treated as generic objects with no named properties and
                // additionalProperties denoting the type
                // maps have 2 child schemas and we are interested the second one of them
                // which is used to determine the additional properties
                let schema_property = ComponentSchema::new(ComponentSchemaProps {
                    type_tree: type_tree
                        .children
                        .as_ref()
                        .expect("ComponentSchema Map type should have children")
                        .get(1)
                        .expect("ComponentSchema Map type should have 2 child"),
                    features: Some(features),
                    description: None,
                    deprecated: None,
                    object_name,
                })?
                .to_token_stream();

                Ok::<_, Diagnostic>(Some(quote! { .additional_properties(#schema_property) }))
            })?;

        tokens.extend(quote! {
            #oapi::oapi::Object::new()
                #additional_properties
                #description_stream
                #deprecated_stream
                #default
        });

        if let Some(example) = example {
            example.try_to_tokens(tokens)?;
        }
        if let Some(nullable) = nullable {
            nullable.try_to_tokens(tokens)?;
        }
        Ok(())
    }

    fn vec_to_tokens(
        tokens: &mut TokenStream,
        mut features: Vec<Feature>,
        type_tree: &TypeTree,
        object_name: &str,
        description_stream: Option<&ComponentDescription<'_>>,
        deprecated_stream: Option<TokenStream>,
    ) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let example = pop_feature!(features => Feature::Example(_));
        let xml = features.extract_vec_xml_feature(type_tree);
        let max_items = pop_feature!(features => Feature::MaxItems(_));
        let min_items = pop_feature!(features => Feature::MinItems(_));
        let nullable = pop_feature!(features => Feature::Nullable(_));
        let default = pop_feature!(features => Feature::Default(_))
            .map(|f| f.try_to_token_stream())
            .transpose()?;

        let child = type_tree
            .children
            .as_ref()
            .expect("ComponentSchema Vec should have children")
            .iter()
            .next()
            .expect("ComponentSchema Vec should have 1 child");

        let unique = matches!(type_tree.generic_type, Some(GenericType::Set));

        // is octet-stream
        let schema = if child
            .path
            .as_ref()
            .map(|path| SchemaType(path).is_byte())
            .unwrap_or(false)
        {
            quote! {
                #oapi::oapi::Object::new()
                    .schema_type(#oapi::oapi::schema::SchemaType::String)
                    .format(#oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Binary))
            }
        } else {
            let component_schema = ComponentSchema::new(ComponentSchemaProps {
                type_tree: child,
                features: Some(features),
                description: None,
                deprecated: None,
                object_name,
            })?;

            let unique = match unique {
                true => quote! {
                    .unique_items(true)
                },
                false => quote! {},
            };

            quote! {
                #oapi::oapi::schema::Array::new(#component_schema)
                #unique
            }
        };

        let validate = |feature: &Feature| {
            let type_path = &**type_tree.path.as_ref().expect("path should not be `None`");
            let schema_type = SchemaType(type_path);
            feature.validate(&schema_type, type_tree)
        };

        tokens.extend(quote! {
            #schema
            #deprecated_stream
            #description_stream
        });

        if let Some(max_items) = max_items {
            validate(&max_items)?;
            tokens.extend(max_items.try_to_token_stream()?)
        }

        if let Some(min_items) = min_items {
            validate(&min_items)?;
            tokens.extend(min_items.try_to_token_stream()?)
        }

        if let Some(default) = default {
            tokens.extend(default.to_token_stream())
        }

        if let Some(example) = example {
            example.try_to_tokens(tokens)?;
        }
        if let Some(xml) = xml {
            xml.try_to_tokens(tokens)?;
        }
        if let Some(nullable) = nullable {
            nullable.try_to_tokens(tokens)?;
        }
        Ok(())
    }

    fn non_generic_to_tokens(
        tokens: &mut TokenStream,
        mut features: Vec<Feature>,
        type_tree: &TypeTree,
        object_name: &str,
        description_stream: Option<&ComponentDescription<'_>>,
        deprecated_stream: Option<TokenStream>,
    ) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let nullable = pop_feature!(features => Feature::Nullable(_))
            .map(|f| f.try_to_token_stream())
            .transpose()?;

        match type_tree.value_type {
            ValueType::Primitive => {
                let type_path = &**type_tree.path.as_ref().expect("path should not be `None`");
                let schema_type = SchemaType(type_path);
                if schema_type.is_unsigned_integer() {
                    // add default minimum feature only when there is no explicit minimum
                    // provided
                    if !features.iter().any(|feature| matches!(&feature, Feature::Minimum(_))) {
                        features.push(Minimum::new(0f64, type_path.span()).into());
                    }
                }

                tokens.extend({
                    let schema_type = schema_type.try_to_token_stream()?;
                    quote! {
                        #oapi::oapi::Object::new().schema_type(#schema_type)
                    }
                });

                let format: SchemaFormat = (type_path).into();
                if format.is_known_format() {
                    let format = format.try_to_token_stream()?;
                    tokens.extend(quote! {
                        .format(#format)
                    })
                }

                description_stream.to_tokens(tokens);
                tokens.extend(deprecated_stream);
                for feature in features.iter().filter(|feature| feature.is_validatable()) {
                    feature.validate(&schema_type, type_tree)?;
                }
                tokens.extend(features.try_to_token_stream()?);

                nullable.to_tokens(tokens);
            }
            ValueType::Object => {
                let is_inline = features.is_inline();

                let default = pop_feature!(features => Feature::Default(_))
                    .map(|f| f.try_to_token_stream())
                    .transpose()?;
                if type_tree.is_object() {
                    let oapi = crate::oapi_crate();
                    let example = features.pop_by(|feature| matches!(feature, Feature::Example(_)));
                    let additional_properties = pop_feature!(features => Feature::AdditionalProperties(_))
                        .unwrap_or_else(|| Feature::AdditionalProperties(AdditionalProperties(true)))
                        .try_to_token_stream()?;

                    tokens.extend(quote! {
                        #oapi::oapi::Object::new()
                            #additional_properties
                            #description_stream
                            #deprecated_stream
                            #default
                    });
                    if let Some(example) = example {
                        example.try_to_tokens(tokens)?;
                    }
                    nullable.to_tokens(tokens);
                } else {
                    let type_path = &**type_tree.path.as_ref().expect("path should not be `None`");
                    if is_inline {
                        let schema = if default.is_some() || nullable.is_some() {
                            quote_spanned! {type_path.span()=>
                                #oapi::oapi::schema::AllOf::new()
                                    #nullable
                                    .item(<#type_path as #oapi::oapi::ToSchema>::to_schema(components))
                                #default
                            }
                        } else {
                            quote_spanned! {type_path.span() =>
                                <#type_path as #oapi::oapi::ToSchema>::to_schema(components)
                            }
                        };
                        schema.to_tokens(tokens);
                    } else {
                        let schema = quote! {
                            #oapi::oapi::RefOr::from(<#type_path as #oapi::oapi::ToSchema>::to_schema(components))
                        };
                        let schema = if default.is_some() || nullable.is_some() {
                            quote! {
                                #oapi::oapi::schema::AllOf::new()
                                    #nullable
                                    .item(#schema)
                                    #default
                            }
                        } else {
                            quote! {
                                #schema
                            }
                        };
                        schema.to_tokens(tokens);
                    }
                }
            }
            ValueType::Tuple => {
                type_tree
                    .children
                    .as_ref()
                    .map(|children| {
                        children
                            .iter()
                            .map(|child| {
                                let features = if child.is_option() {
                                    Some(vec![Feature::Nullable(Nullable::new())])
                                } else {
                                    None
                                };

                                ComponentSchema::new(ComponentSchemaProps {
                                    type_tree: child,
                                    features,
                                    description: None,
                                    deprecated: None,
                                    object_name,
                                })
                            })
                            .collect::<DiagResult<Vec<_>>>()
                    })
                    .transpose()?
                    .map(|children| {
                        let all_of = children.into_iter().fold(
                            quote! { #oapi::oapi::schema::AllOf::new() },
                            |mut all_of, child_tokens| {
                                all_of.extend(quote!( .item(#child_tokens) ));

                                all_of
                            },
                        );
                        quote! {
                            #oapi::oapi::schema::Array::new(#all_of)
                                #nullable
                                #description_stream
                                #deprecated_stream
                        }
                    })
                    .unwrap_or_else(|| quote!(#oapi::oapi::schema::empty()))
                    .to_tokens(tokens);
                tokens.extend(features.try_to_token_stream()?);
            }
        }
        Ok(())
    }

    pub(crate) fn get_deprecated(deprecated: Option<&'c Deprecated>) -> Option<TokenStream> {
        deprecated.map(|deprecated| quote! { .deprecated(#deprecated) })
    }
}

impl ToTokens for ComponentSchema {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens)
    }
}
