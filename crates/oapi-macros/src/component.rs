use proc_macro2::TokenStream;
use quote::{ToTokens, quote, quote_spanned};
use syn::spanned::Spanned;

use crate::doc_comment::CommentAttributes;
use crate::feature::attributes::{AdditionalProperties, Description, Nullable};
use crate::feature::validation::Minimum;
use crate::feature::{Feature, FeaturesExt, IsInline, TryToTokensExt, Validatable, pop_feature};
use crate::schema_type::{SchemaFormat, SchemaType, SchemaTypeInner};
use crate::type_tree::{GenericType, TypeTree, ValueType};
use crate::{Deprecated, DiagResult, Diagnostic, IntoInner, TryToTokens};

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

impl ComponentSchema {
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
            Some(GenericType::Vec | GenericType::LinkedList | GenericType::Set) => {
                ComponentSchema::vec_to_tokens(
                    &mut tokens,
                    features,
                    type_tree,
                    object_name,
                    description,
                    deprecated_stream,
                )?
            }
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
                if !features
                    .iter()
                    .any(|feature| matches!(feature, Feature::Nullable(_)))
                {
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
            Some(
                GenericType::Cow
                | GenericType::Box
                | GenericType::Arc
                | GenericType::Rc
                | GenericType::RefCell,
            ) => {
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

    /// Create `.schema_type(...)` override token stream if nullable is true from given [`SchemaTypeInner`].
    fn get_schema_type_override(
        nullable: Option<Nullable>,
        schema_type_inner: SchemaTypeInner,
    ) -> Option<TokenStream> {
        if let Some(nullable) = nullable {
            let nullable_schema_type = nullable.into_schema_type_token_stream();
            let schema_type = if nullable.value() && !nullable_schema_type.is_empty() {
                let oapi = crate::oapi_crate();
                Some(
                    quote! { #oapi::oapi::schema::SchemaType::from_iter([#schema_type_inner, #nullable_schema_type]) },
                )
            } else {
                None
            };

            schema_type.map(|schema_type| quote! { .schema_type(#schema_type) })
        } else {
            None
        }
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
        let nullable: Option<Nullable> =
            pop_feature!(features => Feature::Nullable(_)).into_inner();
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

        let schema_type =
            ComponentSchema::get_schema_type_override(nullable, SchemaTypeInner::Object);

        tokens.extend(quote! {
            #oapi::oapi::Object::new()
                #schema_type
                #additional_properties
                #description_stream
                #deprecated_stream
                #default
        });

        if let Some(example) = example {
            example.try_to_tokens(tokens)?;
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
        let nullable: Option<Nullable> =
            pop_feature!(features => Feature::Nullable(_)).into_inner();
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

        let component_schema = ComponentSchema::new(ComponentSchemaProps {
            type_tree: child,
            features: Some(features),
            description: None,
            deprecated: None,
            object_name,
        })?
        .to_token_stream();

        let unique = match unique {
            true => quote! {
                .unique_items(true)
            },
            false => quote! {},
        };
        let schema_type =
            ComponentSchema::get_schema_type_override(nullable, SchemaTypeInner::Array);

        let schema = quote! {
            #oapi::oapi::schema::Array::new().items(#component_schema)
            #schema_type
            .items(#component_schema)
            #unique
        };

        let validate = |feature: &Feature| {
            let type_path = &**type_tree.path.as_ref().expect("path should not be `None`");
            let schema_type = SchemaType {
                path: type_path,
                nullable: nullable
                    .map(|nullable| nullable.value())
                    .unwrap_or_default(),
            };
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
        let nullable_feat: Option<Nullable> =
            pop_feature!(features => Feature::Nullable(_)).into_inner();
        let nullable = nullable_feat
            .map(|nullable| nullable.value())
            .unwrap_or_default();

        match type_tree.value_type {
            ValueType::Primitive => {
                let type_path = &**type_tree.path.as_ref().expect("path should not be `None`");
                let schema_type = SchemaType {
                    path: type_path,
                    nullable,
                };
                if schema_type.is_unsigned_integer() {
                    // add default minimum feature only when there is no explicit minimum
                    // provided
                    if !features
                        .iter()
                        .any(|feature| matches!(&feature, Feature::Minimum(_)))
                    {
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
            }
            ValueType::Value => {
                // since OpenAPI 3.1 the type is an array, thus nullable should not be necessary
                // for value type that is going to allow all types of content.
                if type_tree.is_value() {
                    tokens.extend(quote! {
                        #oapi::oapi::Object::new()
                            .schema_type(#oapi::oapi::schema::SchemaType::AnyValue)
                            #description_stream #deprecated_stream
                    })
                }
            }
            ValueType::Object => {
                let is_inline = features.is_inline();

                if type_tree.is_object() {
                    let oapi = crate::oapi_crate();
                    let nullable_schema_type = ComponentSchema::get_schema_type_override(
                        nullable_feat,
                        SchemaTypeInner::Object,
                    );
                    tokens.extend(quote! {
                        #oapi::oapi::Object::new()
                            #nullable_schema_type
                            #description_stream #deprecated_stream
                    })
                } else {
                    let type_path = &**type_tree.path.as_ref().expect("path should not be `None`");
                    let nullable_item = if nullable {
                        Some(
                            quote! { .item(#oapi::oapi::Object::new().schema_type(#oapi::oapi::schema::BasicType::Null)) },
                        )
                    } else {
                        None
                    };
                    if is_inline {
                        let default = pop_feature!(features => Feature::Default(_))
                            .map(|feature| feature.try_to_token_stream())
                            .transpose()?;
                        let schema = if default.is_some() || nullable {
                            quote_spanned! {type_path.span()=>
                                #oapi::oapi::schema::AllOf::new()
                                    #nullable_item
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
                        let default = pop_feature!(features => Feature::Default(_))
                            .map(|feature| feature.try_to_token_stream())
                            .transpose()?;

                        let schema = quote! {
                            #oapi::oapi::RefOr::from(<#type_path as #oapi::oapi::ToSchema>::to_schema(components))
                        };

                        // TODO: refs support `summary` field but currently there is no such field
                        // on schemas more over there is no way to distinct the `summary` from
                        // `description` of the ref. Should we consider supporting the summary?
                        let schema = if default.is_some() || nullable {
                            quote! {
                                #oapi::oapi::schema::AllOf::new()
                                    #nullable_item
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
                        let nullable_schema_type = ComponentSchema::get_schema_type_override(
                            nullable_feat,
                            SchemaTypeInner::Array,
                        );
                        quote! {
                            #oapi::oapi::schema::Array::new().items(#all_of)
                                #nullable_schema_type
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

    pub(crate) fn get_deprecated(deprecated: Option<&Deprecated>) -> Option<TokenStream> {
        deprecated.map(|deprecated| quote! { .deprecated(#deprecated) })
    }
}

impl ToTokens for ComponentSchema {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens)
    }
}
