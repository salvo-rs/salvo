use proc_macro2::TokenStream;
use quote::{ToTokens, quote, quote_spanned};
use syn::spanned::Spanned;

use crate::doc_comment::CommentAttributes;
use crate::feature::attributes::{Description, Inline, Nullable};
use crate::feature::validation::Minimum;
use crate::feature::{Feature, FeaturesExt, IsInline, TryToTokensExt, Validatable, pop_feature};
use crate::schema_type::{SchemaFormat, SchemaType, SchemaTypeInner};
use crate::type_tree::{GenericType, TypeTree, ValueType};
use crate::{Deprecated, DiagResult, IntoInner, TryToTokens};

/// Context for `ComposeSchema` code generation.
///
/// Tracks which type parameters are generic params of the outer struct,
/// so that compose mode can replace them with lookups into the `generics` vector.
#[derive(Debug, Clone)]
pub(crate) struct ComposeContext {
    /// The identifier for the generics vector parameter (e.g., `__compose_generics`).
    pub(crate) generics_ident: proc_macro2::Ident,
    /// Generic type parameter names in declaration order (e.g., `["T", "U"]`).
    pub(crate) params: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct ComponentSchemaProps<'c> {
    pub(crate) type_tree: &'c TypeTree<'c>,
    pub(crate) features: Option<Vec<Feature>>,
    pub(crate) description: Option<&'c ComponentDescription<'c>>,
    pub(crate) deprecated: Option<&'c Deprecated>,
    pub(crate) object_name: &'c str,
    pub(crate) compose_context: Option<&'c ComposeContext>,
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
    pub(crate) fn new(schema_props: ComponentSchemaProps) -> DiagResult<Self> {
        Self::new_inner(schema_props, true, None)
    }

    /// Preserve explicit inlining when the original Rust type is available.
    pub(crate) fn for_type(schema_props: ComponentSchemaProps, ty: &syn::Type) -> DiagResult<Self> {
        let force_inline = schema_props
            .features
            .as_ref()
            .is_some_and(|features| features.is_inline());
        Self::new_inner(schema_props, true, force_inline.then_some(ty))
    }

    /// Create a `ComponentSchema` for use in parameter contexts.
    ///
    /// When used for parameters, `Option<T>` means the parameter is optional (not required),
    /// NOT that the parameter value is nullable. The `required` status is handled separately
    /// by the parameter derive code.
    pub(crate) fn for_params(
        schema_props: ComponentSchemaProps,
        ty: &syn::Type,
    ) -> DiagResult<Self> {
        let force_inline = schema_props
            .features
            .as_ref()
            .is_some_and(|features| features.is_inline());
        Self::new_inner(schema_props, false, force_inline.then_some(ty))
    }

    fn new_inner(
        ComponentSchemaProps {
            type_tree,
            features,
            description,
            deprecated,
            object_name,
            compose_context,
        }: ComponentSchemaProps,
        option_is_nullable: bool,
        inline_type: Option<&syn::Type>,
    ) -> DiagResult<Self> {
        let mut tokens = TokenStream::new();
        let mut features = features.unwrap_or_default();
        let deprecated_stream = Self::get_deprecated(deprecated);

        match type_tree.generic_type {
            Some(GenericType::Map) => Self::map_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
                compose_context,
                inline_type,
            )?,
            Some(GenericType::Vec | GenericType::LinkedList | GenericType::Set) => {
                Self::vec_to_tokens(
                    &mut tokens,
                    features,
                    type_tree,
                    object_name,
                    description,
                    deprecated_stream,
                    compose_context,
                    inline_type,
                )?
            }
            #[cfg(feature = "smallvec")]
            Some(GenericType::SmallVec) => Self::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
                compose_context,
                inline_type,
            )?,
            Some(GenericType::Option) => {
                // Add nullable feature if not already exists. Option is always nullable
                // UNLESS we are in a parameter context where Option means "not required".
                if option_is_nullable
                    && !features
                        .iter()
                        .any(|feature| matches!(feature, Feature::Nullable(_)))
                {
                    features.push(Nullable::new().into());
                }

                Self::new_inner(
                    ComponentSchemaProps {
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
                        compose_context,
                    },
                    option_is_nullable,
                    inline_type.and_then(|ty| inline_child_type(ty, 0)),
                )?
                .to_tokens(&mut tokens);
            }
            Some(
                GenericType::Cow
                | GenericType::Box
                | GenericType::Arc
                | GenericType::Rc
                | GenericType::RefCell,
            ) => {
                Self::new_inner(
                    ComponentSchemaProps {
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
                        compose_context,
                    },
                    option_is_nullable,
                    inline_type.and_then(|ty| inline_child_type(ty, 0)),
                )?
                .to_tokens(&mut tokens);
            }
            None => Self::non_generic_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description,
                deprecated_stream,
                compose_context,
                inline_type,
            )?,
        }

        Ok(Self { tokens })
    }

    /// Create `.schema_type(...)` override token stream if nullable is true from given
    /// [`SchemaTypeInner`].
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
        compose_context: Option<&ComposeContext>,
        inline_type: Option<&syn::Type>,
    ) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let example = features.pop_by(|feature| matches!(feature, Feature::Example(_)));
        let additional_properties = pop_feature!(features => Feature::AdditionalProperties(_));
        let nullable: Option<Nullable> =
            pop_feature!(features => Feature::Nullable(_)).into_inner();
        let default = pop_feature!(features => Feature::Default(_))
            .map(|f| f.try_to_token_stream())
            .transpose()?;

        // Generate property_names from the map key type (first child)
        let children = type_tree
            .children
            .as_ref()
            .expect("ComponentSchema Map type should have children");
        let key_type = children
            .first()
            .expect("ComponentSchema Map type should have 2 children, getting first");
        let mut property_name_features = features.clone();
        property_name_features.push(Feature::Inline(Inline(true)));
        let property_names_schema = Self::new_inner(
            ComponentSchemaProps {
                type_tree: key_type,
                features: Some(property_name_features),
                description: None,
                deprecated: None,
                object_name,
                compose_context,
            },
            true,
            inline_type.and_then(|ty| inline_child_type(ty, 0)),
        )?
        .to_token_stream();

        let additional_properties =
            if let Some(additional_properties) = additional_properties.as_ref() {
                Some(additional_properties.try_to_token_stream()?)
            } else {
                // Maps are treated as generic objects with no named properties and
                // additionalProperties denoting the type.
                // Maps have 2 child schemas and we are interested in the second one of them
                // which is used to determine the additional properties.
                let schema_property = Self::new_inner(
                    ComponentSchemaProps {
                        type_tree: children
                            .get(1)
                            .expect("ComponentSchema Map type should have 2 children"),
                        features: Some(features),
                        description: None,
                        deprecated: None,
                        object_name,
                        compose_context,
                    },
                    true,
                    inline_type.and_then(|ty| inline_child_type(ty, 1)),
                )?
                .to_token_stream();

                Some(quote! { .additional_properties(#schema_property) })
            };

        let schema_type = Self::get_schema_type_override(nullable, SchemaTypeInner::Object);

        tokens.extend(quote! {
            #oapi::oapi::Object::new()
                #schema_type
                .property_names(#oapi::oapi::schema::serialized_map_key_schema((#property_names_schema).into(), components))
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
        compose_context: Option<&ComposeContext>,
        inline_type: Option<&syn::Type>,
    ) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let examples: Vec<_> = features
            .extract_if(.., |feature| {
                matches!(feature, Feature::Example(_) | Feature::Examples(_))
            })
            .collect();
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

        #[cfg(feature = "smallvec")]
        let opaque_storage =
            type_tree.generic_type == Some(GenericType::SmallVec) && child.array_len.is_none();
        #[cfg(feature = "smallvec")]
        let child = if type_tree.generic_type == Some(GenericType::SmallVec) && !opaque_storage {
            child
                .children
                .as_ref()
                .and_then(|children| children.first())
                .unwrap_or(child)
        } else {
            child
        };

        let unique = matches!(type_tree.generic_type, Some(GenericType::Set));

        let inline_child = inline_type.and_then(|ty| inline_child_type(ty, 0));
        #[cfg(feature = "smallvec")]
        let inline_child = if type_tree.generic_type == Some(GenericType::SmallVec) {
            inline_child.and_then(|ty| inline_child_type(ty, 0))
        } else {
            inline_child
        };

        #[cfg(feature = "smallvec")]
        let opaque_item = if opaque_storage {
            let path = type_tree.path.as_ref().expect("SmallVec type path");
            let item = quote!(<#path as ::std::iter::IntoIterator>::Item);
            let inline = features.is_inline();
            let no_recursion = pop_feature!(features => Feature::NoRecursion(_)).is_some();
            let _ = pop_feature!(features => Feature::Inline(_));
            let schema = if no_recursion {
                quote! {{
                    let name = #oapi::oapi::naming::assign_name::<#item>(#oapi::oapi::naming::NameRule::Auto);
                    #oapi::oapi::RefOr::Ref(#oapi::oapi::Ref::from_schema_name(name))
                }}
            } else if inline {
                quote!(<#item as #oapi::oapi::ComposeSchema>::compose(components, ::std::vec::Vec::new()))
            } else {
                quote!(<#item as #oapi::oapi::ToSchema>::to_schema(components))
            };
            let annotations = features.try_to_token_stream()?;
            Some(
                quote!(#oapi::oapi::schema::annotate_schema((#schema).into(),
                #oapi::oapi::Object::new().schema_type(#oapi::oapi::SchemaType::AnyValue) #annotations)),
            )
        } else {
            None
        };

        let component_schema = Self::new_inner(
            ComponentSchemaProps {
                type_tree: child,
                features: Some(features),
                description: None,
                deprecated: None,
                object_name,
                compose_context,
            },
            true,
            inline_child,
        )?
        .to_token_stream();
        #[cfg(feature = "smallvec")]
        let component_schema = opaque_item.unwrap_or(component_schema);

        let unique = match unique {
            true => quote! {
                .unique_items(true)
            },
            false => quote! {},
        };
        let schema_type = Self::get_schema_type_override(nullable, SchemaTypeInner::Array);

        let schema = quote! {
            #oapi::oapi::schema::Array::new().items(#component_schema)
            #schema_type
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

        for example in examples {
            example.try_to_tokens(tokens)?;
        }
        if let Some(syn::Type::Array(array)) = inline_type.map(unwrapped_type) {
            let len = &array.len;
            tokens.extend(quote! { .min_items(#len).max_items(#len) });
        } else if let Some(len) = type_tree.array_len {
            tokens.extend(quote! { .min_items(#len).max_items(#len) });
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
        compose_context: Option<&ComposeContext>,
        inline_type: Option<&syn::Type>,
    ) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let annotations: Vec<_> = if type_tree.value_type != ValueType::Primitive {
            features
                .extract_if(.., |feature| {
                    matches!(
                        feature,
                        Feature::ReadOnly(_)
                            | Feature::WriteOnly(_)
                            | Feature::Deprecated(_)
                            | Feature::Example(_)
                            | Feature::Examples(_)
                            | Feature::ContentEncoding(_)
                            | Feature::ContentMediaType(_)
                            | Feature::Title(_)
                            | Feature::Default(_)
                            | Feature::XmlAttr(_)
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let annotation_tokens = annotations.try_to_token_stream()?;
        let annotation_deprecated = (type_tree.value_type != ValueType::Primitive)
            .then(|| deprecated_stream.clone())
            .flatten();
        let nullable_feat: Option<Nullable> =
            pop_feature!(features => Feature::Nullable(_)).into_inner();
        let nullable = nullable_feat
            .map(|nullable| nullable.value())
            .unwrap_or_default();
        // Nullable children can already accept null (for example nested Option).
        // Such alternatives form an inclusive union, not an exclusive oneOf.
        let union = if nullable {
            quote! { #oapi::oapi::schema::AnyOf }
        } else {
            quote! { #oapi::oapi::schema::OneOf }
        };

        // In compose mode, check if this type is a generic param that should
        // be looked up from the compose generics vector.
        if let Some(ctx) = compose_context
            && let Some(idx) = type_tree
                .path
                .as_ref()
                .and_then(|p| p.segments.last())
                .and_then(|seg| {
                    if p_is_single_segment(type_tree.path.as_ref()) {
                        ctx.params.iter().position(|p| seg.ident == *p)
                    } else {
                        None
                    }
                })
        {
            let generics_ident = &ctx.generics_ident;
            let nullable_item = if nullable {
                Some(
                    quote! { .item(#oapi::oapi::Object::new().schema_type(#oapi::oapi::schema::BasicType::Null)) },
                )
            } else {
                None
            };
            let default = pop_feature!(features => Feature::Default(_))
                .map(|feature| feature.try_to_token_stream())
                .transpose()?;
            let title = pop_feature!(features => Feature::Title(_))
                .map(|feature| feature.try_to_token_stream())
                .transpose()?;
            let description_tokens = description_stream.to_token_stream();
            let has_description = !description_tokens.is_empty();

            let generic_path = type_tree.path.as_ref().expect("generic parameter path");
            let schema = quote! { #generics_ident.get(#idx).and_then(::std::clone::Clone::clone)
            .unwrap_or_else(|| <#generic_path as #oapi::oapi::ComposeSchema>::compose(components, ::std::vec::Vec::new())) };

            let schema = if default.is_some() || nullable {
                quote! {
                    #union::new()
                        #nullable_item
                        .item(#schema)
                        #default
                }
            } else {
                schema
            };

            let schema = if title.is_some() || has_description {
                quote! {
                    #oapi::oapi::schema::AllOf::new()
                        .item(#schema)
                        .item(#oapi::oapi::Object::new().schema_type(#oapi::oapi::schema::SchemaType::AnyValue) #title #description_stream)
                }
            } else {
                schema
            };

            let schema = if !annotations.is_empty() || annotation_deprecated.is_some() {
                quote! { #oapi::oapi::schema::annotate_schema((#schema).into(),
                #oapi::oapi::Object::new().schema_type(#oapi::oapi::SchemaType::AnyValue)
                #annotation_tokens #annotation_deprecated) }
            } else {
                schema
            };
            schema.to_tokens(tokens);
            return Ok(());
        }

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

                let format: SchemaFormat = type_path.into();
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
                // primitive types are not recursive, consume no_recursion
                let _ = pop_feature!(features => Feature::NoRecursion(_));
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
                let no_recursion = pop_feature!(features => Feature::NoRecursion(_)).is_some();

                if type_tree.is_object() {
                    let oapi = crate::oapi_crate();
                    let nullable_schema_type =
                        Self::get_schema_type_override(nullable_feat, SchemaTypeInner::Object);
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

                    let schema_type = SchemaType {
                        path: type_path,
                        nullable,
                    };
                    let is_inline =
                        is_inline && (inline_type.is_some() || schema_type.is_primitive());

                    if no_recursion {
                        // When no_recursion is set, emit a $ref without calling to_schema()
                        // to prevent infinite recursion in schema generation.
                        let default = pop_feature!(features => Feature::Default(_))
                            .map(|feature| feature.try_to_token_stream())
                            .transpose()?;

                        let schema = quote! {
                            {
                                let name = #oapi::oapi::naming::assign_name::<#type_path>(#oapi::oapi::naming::NameRule::Auto);
                                #oapi::oapi::RefOr::Ref(#oapi::oapi::Ref::new(format!("#/components/schemas/{}", name)))
                            }
                        };

                        let schema = if default.is_some() || nullable {
                            quote! {
                                #union::new()
                                    #nullable_item
                                    .item(#schema)
                                    #default
                            }
                        } else {
                            schema
                        };
                        schema.to_tokens(tokens);
                    } else {
                        if is_inline {
                            let default = pop_feature!(features => Feature::Default(_))
                                .map(|feature| feature.try_to_token_stream())
                                .transpose()?;
                            let title = pop_feature!(features => Feature::Title(_))
                                .map(|feature| feature.try_to_token_stream())
                                .transpose()?;
                            let description_tokens = description_stream.to_token_stream();
                            let has_description = !description_tokens.is_empty();

                            let inline_schema = if let Some(ty) = inline_type {
                                build_inline_compose_block(ty, &oapi)
                            } else {
                                quote! { <#type_path as #oapi::oapi::ToSchema>::to_schema(components) }
                            };
                            let schema = if default.is_some() || nullable {
                                quote_spanned! {type_path.span()=>
                                    #union::new()
                                        #nullable_item
                                        .item(#inline_schema)
                                    #default
                                }
                            } else {
                                quote_spanned! {type_path.span() =>
                                    #inline_schema
                                }
                            };

                            // If the inlined field has a title or description, wrap in allOf
                            // to attach the metadata without violating the OpenAPI spec.
                            let schema = if title.is_some() || has_description {
                                quote! {
                                    #oapi::oapi::schema::AllOf::new()
                                        .item(#schema)
                                        .item(#oapi::oapi::Object::new().schema_type(#oapi::oapi::schema::SchemaType::AnyValue) #title #description_stream)
                                }
                            } else {
                                schema
                            };

                            schema.to_tokens(tokens);
                        } else {
                            let default = pop_feature!(features => Feature::Default(_))
                                .map(|feature| feature.try_to_token_stream())
                                .transpose()?;
                            let title = pop_feature!(features => Feature::Title(_))
                                .map(|feature| feature.try_to_token_stream())
                                .transpose()?;
                            let description_tokens = description_stream.to_token_stream();
                            let has_description = !description_tokens.is_empty();

                            let schema = if type_path.is_ident("Self") && compose_context.is_some()
                            {
                                let generics = &compose_context
                                    .expect("checked compose context")
                                    .generics_ident;
                                quote! {{
                                    let __salvo_self_arguments = #generics.iter().map(|schema| schema.clone()
                                        .unwrap_or_else(|| #oapi::oapi::RefOr::from(true))).collect();
                                    <Self as #oapi::oapi::ComposeSchema>::compose(components, __salvo_self_arguments)
                                }}
                            } else if let Some(context) = compose_context
                                .filter(|context| contains_compose_parameter(type_tree, context))
                            {
                                let children = type_path
                                    .segments
                                    .last()
                                    .and_then(|segment| match &segment.arguments {
                                        syn::PathArguments::AngleBracketed(args) => {
                                            Some(&args.args)
                                        }
                                        _ => None,
                                    })
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|argument| match argument {
                                        syn::GenericArgument::Type(ty) => Some(ty),
                                        _ => None,
                                    })
                                    .map(|ty| {
                                        let child = TypeTree::from_type(ty)?;
                                        if !contains_compose_parameter(&child, context) {
                                            // A bare identifier may be a const argument.
                                            // Resolve concrete/default arguments from the
                                            // actual implementation instead of guessing.
                                            return Ok(quote!(::std::option::Option::None));
                                        }
                                        let schema = Self::new(ComponentSchemaProps {
                                            type_tree: &child,
                                            features: None,
                                            description: None,
                                            deprecated: None,
                                            object_name,
                                            compose_context: Some(context),
                                        })?
                                        .to_token_stream();
                                        Ok(quote!(::std::option::Option::Some((
                                            ::std::any::type_name::<#ty>(), (#schema).into()))))
                                    })
                                    .collect::<DiagResult<Vec<_>>>()?;
                                let mut source_path = Vec::new();
                                if type_path.leading_colon.is_some() {
                                    source_path.push(String::new());
                                }
                                source_path.extend(
                                    type_path
                                        .segments
                                        .iter()
                                        .map(|segment| segment.ident.to_string()),
                                );
                                let args = quote::format_ident!(
                                    "__salvo_compose_children",
                                    span = proc_macro2::Span::mixed_site()
                                );
                                quote! {{ let #args = ::std::vec![#(#children),*];
                                <#type_path as #oapi::oapi::ComposeSchema>::compose_with_bindings(
                                    components, ::std::module_path!(), &[#(#source_path),*], #args) }}
                            } else {
                                quote! { #oapi::oapi::RefOr::from(<#type_path as #oapi::oapi::ToSchema>::to_schema(components)) }
                            };

                            // TODO: refs support `summary` field but currently there is no such
                            // field on schemas more over there is no
                            // way to distinct the `summary` from
                            // `description` of the ref. Should we consider supporting the summary?
                            let schema = if default.is_some() || nullable {
                                let schema = quote! {
                                    #union::new()
                                        #nullable_item
                                        .item(#schema)
                                        #default
                                };
                                // If $ref has title or description, wrap further in allOf
                                if title.is_some() || has_description {
                                    quote! {
                                        #oapi::oapi::schema::AllOf::new()
                                            .item(#schema)
                                            .item(#oapi::oapi::Object::new().schema_type(#oapi::oapi::schema::SchemaType::AnyValue) #title #description_stream)
                                    }
                                } else {
                                    schema
                                }
                            } else if title.is_some() || has_description {
                                // Wrap $ref in allOf to attach title/description without
                                // violating the OpenAPI spec (no extra properties on $ref).
                                quote! {
                                    #oapi::oapi::schema::AllOf::new()
                                        .item(#schema)
                                        .item(#oapi::oapi::Object::new().schema_type(#oapi::oapi::schema::SchemaType::AnyValue) #title #description_stream)
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
            }
            ValueType::Tuple => {
                type_tree
                    .children
                    .as_ref()
                    .map(|children| {
                        children
                            .iter()
                            .enumerate()
                            .map(|(index, child)| {
                                let features = if child.is_option() {
                                    Some(vec![Feature::Nullable(Nullable::new())])
                                } else {
                                    None
                                };

                                let mut features = features.unwrap_or_default();
                                if inline_type.is_some() { features.push(Feature::Inline(Inline(true))); }
                                Self::new_inner(ComponentSchemaProps {
                                    type_tree: child,
                                    features: Some(features),
                                    description: None,
                                    deprecated: None,
                                    object_name,
                                    compose_context,
                                }, true, inline_type.and_then(|ty| inline_child_type(ty, index)))
                            })
                            .collect::<DiagResult<Vec<_>>>()
                    })
                    .transpose()?
                    .map(|children| {
                        let len = children.len();
                        let fixed_len = inline_type.map(|_| quote! { .min_items(#len).max_items(#len) });
                        let prefix_items = children.into_iter().collect::<Vec<_>>();
                        let nullable_schema_type =
                            Self::get_schema_type_override(nullable_feat, SchemaTypeInner::Array);
                        quote! {
                            #oapi::oapi::schema::Array::new()
                                .items(#oapi::oapi::schema::ArrayItems::False)
                                .min_items(#len)
                                .prefix_items([#(
                                    Into::<#oapi::oapi::RefOr<#oapi::oapi::schema::Schema>>::into(#prefix_items)
                                ),*])
                                #fixed_len
                                #nullable_schema_type
                                #description_stream
                                #deprecated_stream
                        }
                    })
                    .unwrap_or_else(|| {
                        if inline_type.is_some() {
                            quote!(#oapi::oapi::Object::new().schema_type(#oapi::oapi::BasicType::Null))
                        } else {
                            quote!(#oapi::oapi::schema::empty())
                        }
                    })
                    .to_tokens(tokens);
                tokens.extend(features.try_to_token_stream()?);
            }
        }
        if !annotations.is_empty() || annotation_deprecated.is_some() {
            let schema = tokens.clone();
            *tokens = quote! { #oapi::oapi::schema::annotate_schema((#schema).into(),
            #oapi::oapi::Object::new().schema_type(#oapi::oapi::SchemaType::AnyValue)
            #annotation_tokens #annotation_deprecated) };
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

/// Check if a path has only a single segment (e.g., `T` vs `std::string::String`).
fn p_is_single_segment(path: Option<&std::borrow::Cow<'_, syn::Path>>) -> bool {
    path.map(|p| p.segments.len() == 1 && p.leading_colon.is_none())
        .unwrap_or(false)
}

// Keep structural syntax alongside TypeTree while recursively generating
// explicitly inline parameter schemas and their validation features.
fn unwrapped_type(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Reference(reference) => unwrapped_type(&reference.elem),
        syn::Type::Paren(paren) => unwrapped_type(&paren.elem),
        syn::Type::Group(group) => unwrapped_type(&group.elem),
        _ => ty,
    }
}

fn inline_child_type(ty: &syn::Type, index: usize) -> Option<&syn::Type> {
    match unwrapped_type(ty) {
        syn::Type::Array(array) => (index == 0).then_some(&*array.elem),
        syn::Type::Slice(slice) => (index == 0).then_some(&*slice.elem),
        syn::Type::Tuple(tuple) => tuple.elems.iter().nth(index),
        syn::Type::Path(path) => {
            path.path
                .segments
                .last()
                .and_then(|segment| match &segment.arguments {
                    syn::PathArguments::AngleBracketed(args) => args
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            syn::GenericArgument::Type(ty) => Some(ty),
                            _ => None,
                        })
                        .nth(index),
                    _ => None,
                })
        }
        _ => None,
    }
}

/// Compose an explicitly inline Rust type without losing its structural syntax.
/// Each child is evaluated first to keep mutable component borrows independent.
pub(crate) fn build_inline_compose_block(ty: &syn::Type, oapi: &syn::Path) -> TokenStream {
    match ty {
        syn::Type::Reference(reference) => build_inline_compose_block(&reference.elem, oapi),
        syn::Type::Paren(paren) => build_inline_compose_block(&paren.elem, oapi),
        syn::Type::Group(group) => build_inline_compose_block(&group.elem, oapi),
        syn::Type::Array(array) => {
            let item = build_inline_compose_block(&array.elem, oapi);
            let ident =
                quote::format_ident!("__inline_item", span = proc_macro2::Span::mixed_site());
            let len = &array.len;
            quote_spanned! {ty.span()=>
                {
                    let #ident = #item;
                    #oapi::oapi::RefOr::from(#oapi::oapi::schema::Array::new()
                        .items(#ident).min_items(#len).max_items(#len))
                }
            }
        }
        syn::Type::Slice(slice) => {
            let item = build_inline_compose_block(&slice.elem, oapi);
            let ident =
                quote::format_ident!("__inline_item", span = proc_macro2::Span::mixed_site());
            quote_spanned! {ty.span()=>
                {
                    let #ident = #item;
                    #oapi::oapi::RefOr::from(#oapi::oapi::schema::Array::new().items(#ident))
                }
            }
        }
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => quote_spanned! {ty.span()=>
            #oapi::oapi::RefOr::from(#oapi::oapi::Object::new().schema_type(#oapi::oapi::BasicType::Null))
        },
        syn::Type::Tuple(tuple) => {
            let count = tuple.elems.len();
            let bindings = tuple.elems.iter().enumerate().map(|(idx, elem)| {
                let ident = quote::format_ident!(
                    "__inline_arg_{}",
                    idx,
                    span = proc_macro2::Span::mixed_site()
                );
                let value = build_inline_compose_block(elem, oapi);
                quote! { let #ident = #value; }
            });
            let args = (0..count).map(|idx| {
                quote::format_ident!(
                    "__inline_arg_{}",
                    idx,
                    span = proc_macro2::Span::mixed_site()
                )
            });
            quote_spanned! {ty.span()=>
                {
                    #(#bindings)*
                    #oapi::oapi::RefOr::from(#oapi::oapi::schema::Array::new()
                        .items(#oapi::oapi::schema::ArrayItems::False)
                        .prefix_items([#(#args),*]).min_items(#count).max_items(#count))
                }
            }
        }
        syn::Type::Path(_) => quote_spanned! {ty.span()=>
            <#ty as #oapi::oapi::ComposeSchema>::compose(components, ::std::vec::Vec::new())
        },
        _ => quote_spanned! {ty.span()=>
            <#ty as #oapi::oapi::ComposeSchema>::compose(components, ::std::vec::Vec::new())
        },
    }
}

fn contains_compose_parameter(tree: &TypeTree<'_>, context: &ComposeContext) -> bool {
    tree.path.as_ref().is_some_and(|path| {
        path.segments.len() == 1
            && context
                .params
                .iter()
                .any(|name| path.segments[0].ident == name)
    }) || tree.children.as_ref().is_some_and(|children| {
        children
            .iter()
            .any(|child| contains_compose_parameter(child, context))
    })
}
