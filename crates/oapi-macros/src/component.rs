use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;

use crate::doc_comment::CommentAttributes;
use crate::feature::{
    pop_feature, AdditionalProperties, Feature, FeaturesExt, IsInline, Minimum, Nullable, ToTokensExt, Validatable,
};
use crate::schema_type::{SchemaFormat, SchemaType};
use crate::type_tree::{GenericType, TypeTree, ValueType};
use crate::Deprecated;

#[derive(Debug)]
pub(crate) struct ComponentSchemaProps<'c> {
    pub(crate) type_tree: &'c TypeTree<'c>,
    pub(crate) features: Option<Vec<Feature>>,
    pub(crate) description: Option<&'c CommentAttributes>,
    pub(crate) deprecated: Option<&'c Deprecated>,
    pub(crate) object_name: &'c str,
    pub(crate) type_definition: bool,
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
            type_definition,
        }: ComponentSchemaProps,
    ) -> Self {
        let mut tokens = TokenStream::new();
        let mut features = features.unwrap_or(Vec::new());
        let deprecated_stream = ComponentSchema::get_deprecated(deprecated);
        let description_stream = ComponentSchema::get_description(description);

        match type_tree.generic_type {
            Some(GenericType::Map) => ComponentSchema::map_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description_stream,
                deprecated_stream,
                type_definition,
            ),
            Some(GenericType::Vec) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description_stream,
                deprecated_stream,
                type_definition,
            ),
            Some(GenericType::LinkedList) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description_stream,
                deprecated_stream,
                type_definition,
            ),
            Some(GenericType::Set) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description_stream,
                deprecated_stream,
                type_definition,
            ),
            #[cfg(feature = "smallvec")]
            Some(GenericType::SmallVec) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description_stream,
                deprecated_stream,
                type_definition,
            ),
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
                    type_definition,
                })
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
                    type_definition,
                })
                .to_tokens(&mut tokens);
            }
            None => ComponentSchema::non_generic_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description_stream,
                deprecated_stream,
                type_definition,
            ),
        }

        Self { tokens }
    }

    fn map_to_tokens(
        tokens: &mut TokenStream,
        mut features: Vec<Feature>,
        type_tree: &TypeTree,
        object_name: &str,
        description_stream: Option<TokenStream>,
        deprecated_stream: Option<TokenStream>,
        type_definition: bool,
    ) {
        let oapi = crate::oapi_crate();
        let example = features.pop_by(|feature| matches!(feature, Feature::Example(_)));
        let additional_properties = pop_feature!(features => Feature::AdditionalProperties(_));
        let nullable = pop_feature!(features => Feature::Nullable(_));
        let default = pop_feature!(features => Feature::Default(_));

        let additional_properties = additional_properties
            .as_ref()
            .map(ToTokens::to_token_stream)
            .unwrap_or_else(|| {
                // Maps are treated as generic objects with no named properties and
                // additionalProperties denoting the type
                // maps have 2 child schemas and we are interested the second one of them
                // which is used to determine the additional properties
                let schema_property = ComponentSchema::new(ComponentSchemaProps {
                    type_tree: type_tree
                        .children
                        .as_ref()
                        .expect("ComponentSchema Map type should have children")
                        .iter()
                        .nth(1)
                        .expect("ComponentSchema Map type should have 2 child"),
                    features: Some(features),
                    description: None,
                    deprecated: None,
                    object_name,
                    type_definition,
                });

                quote! { .additional_properties(#schema_property) }
            });

        tokens.extend(quote! {
            #oapi::oapi::Object::new()
                #additional_properties
                #description_stream
                #deprecated_stream
                #default
        });

        example.to_tokens(tokens);
        nullable.to_tokens(tokens)
    }

    fn vec_to_tokens(
        tokens: &mut TokenStream,
        mut features: Vec<Feature>,
        type_tree: &TypeTree,
        object_name: &str,
        description_stream: Option<TokenStream>,
        deprecated_stream: Option<TokenStream>,
        type_definition: bool,
    ) {
        let oapi = crate::oapi_crate();
        let example = pop_feature!(features => Feature::Example(_));
        let xml = features.extract_vec_xml_feature(type_tree);
        let max_items = pop_feature!(features => Feature::MaxItems(_));
        let min_items = pop_feature!(features => Feature::MinItems(_));
        let nullable = pop_feature!(features => Feature::Nullable(_));
        let default = pop_feature!(features => Feature::Default(_));

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
                type_definition,
            });

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
            feature.validate(&schema_type, type_tree);
        };

        tokens.extend(quote! {
            #schema
            #deprecated_stream
            #description_stream
        });

        if let Some(max_items) = max_items {
            validate(&max_items);
            tokens.extend(max_items.to_token_stream())
        }

        if let Some(min_items) = min_items {
            validate(&min_items);
            tokens.extend(min_items.to_token_stream())
        }

        if let Some(default) = default {
            tokens.extend(default.to_token_stream())
        }

        example.to_tokens(tokens);
        xml.to_tokens(tokens);
        nullable.to_tokens(tokens);
    }

    fn non_generic_to_tokens(
        tokens: &mut TokenStream,
        mut features: Vec<Feature>,
        type_tree: &TypeTree,
        object_name: &str,
        description_stream: Option<TokenStream>,
        deprecated_stream: Option<TokenStream>,
        type_definition: bool,
    ) {
        let nullable = pop_feature!(features => Feature::Nullable(_));
        let oapi = crate::oapi_crate();

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

                tokens.extend(quote! {
                    #oapi::oapi::Object::new().schema_type(#schema_type)
                });

                let format: SchemaFormat = (type_path).into();
                if format.is_known_format() {
                    tokens.extend(quote! {
                        .format(#format)
                    })
                }

                tokens.extend(description_stream);
                tokens.extend(deprecated_stream);
                for feature in features.iter().filter(|feature| feature.is_validatable()) {
                    feature.validate(&schema_type, type_tree);
                }
                tokens.extend(features.to_token_stream());
                nullable.to_tokens(tokens);
            }
            ValueType::Object => {
                let is_inline = features.is_inline();

                if type_tree.is_object() {
                    let oapi = crate::oapi_crate();
                    let example = features.pop_by(|feature| matches!(feature, Feature::Example(_)));
                    let additional_properties = pop_feature!(features => Feature::AdditionalProperties(_))
                        .unwrap_or_else(|| Feature::AdditionalProperties(AdditionalProperties(true)));
                    let nullable = pop_feature!(features => Feature::Nullable(_));
                    let default = pop_feature!(features => Feature::Default(_));

                    tokens.extend(quote! {
                        #oapi::oapi::Object::new()
                            #additional_properties
                            #description_stream
                            #deprecated_stream
                            #default
                    });
                    example.to_tokens(tokens);
                    nullable.to_tokens(tokens)
                } else {
                    let type_path = &**type_tree.path.as_ref().expect("path should not be `None`");
                    let schema = if type_definition {
                        quote! {
                            if std::any::TypeId::of::<#type_path>() == std::any::TypeId::of::<Self>() {
                                #oapi::oapi::RefOr::<#oapi::oapi::Schema>::Ref(#oapi::oapi::schema::Ref::new("#"))
                            } else {
                                #oapi::oapi::RefOr::from(<#type_path as #oapi::oapi::ToSchema>::to_schema(components))
                            }
                        }
                    } else {
                        quote! {
                            <#type_path as #oapi::oapi::ToSchema>::to_schema(components)
                        }
                    };
                    if is_inline {
                        let default = pop_feature!(features => Feature::Default(_));
                        let schema = if default.is_some() || nullable.is_some() {
                            quote_spanned! {type_path.span()=>
                                #oapi::oapi::schema::AllOf::new()
                                    #nullable
                                    .item(#schema)
                                #default
                            }
                        } else {
                            quote_spanned! {type_path.span() =>
                                #schema
                            }
                        };
                        schema.to_tokens(tokens);
                    } else {
                        let default = pop_feature!(features => Feature::Default(_));
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
                        let all_of =
                            children
                                .iter()
                                .fold(quote! { #oapi::oapi::schema::AllOf::new() }, |mut all_of, child| {
                                    let features = if child.is_option() {
                                        Some(vec![Feature::Nullable(Nullable::new())])
                                    } else {
                                        None
                                    };

                                    let item = ComponentSchema::new(ComponentSchemaProps {
                                        type_tree: child,
                                        features,
                                        description: None,
                                        deprecated: None,
                                        object_name,
                                        type_definition,
                                    });
                                    all_of.extend(quote!( .item(#item) ));

                                    all_of
                                });
                        quote! {
                            #oapi::oapi::schema::Array::new(#all_of)
                                #nullable
                                #description_stream
                                #deprecated_stream
                        }
                    })
                    .unwrap_or_else(|| quote!(#oapi::oapi::schema::empty()))
                    .to_tokens(tokens);
                tokens.extend(features.to_token_stream());
            }
        }
    }

    pub(crate) fn get_description(comments: Option<&'c CommentAttributes>) -> Option<TokenStream> {
        comments
            .and_then(|comments| {
                let comment = CommentAttributes::as_formatted_string(comments);
                if comment.is_empty() {
                    None
                } else {
                    Some(comment)
                }
            })
            .map(|description| quote! { .description(#description) })
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
