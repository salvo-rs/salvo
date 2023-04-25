use std::borrow::Cow;

use proc_macro2::{Ident, Span, TokenStream};
use proc_macro_error::{abort, abort_call_site};
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{Attribute, GenericArgument, Path, PathArguments, PathSegment, Type, TypePath};

use crate::doc_comment::CommentAttributes;
use crate::schema_type::SchemaFormat;
use crate::{schema_type::SchemaType, Deprecated};

use self::features::{pop_feature, Feature, FeaturesExt, IsInline, Minimum, Nullable, ToTokensExt, Validatable};
use self::schema::format_path_ref;
use self::serde::{RenameRule, SerdeContainer, SerdeValue};

pub mod features;
pub mod schema;
pub mod serde;

/// Check whether either serde `container_rule` or `field_rule` has _`default`_ attribute set.
#[inline]
fn is_default(container_rules: &Option<&SerdeContainer>, field_rule: &Option<&SerdeValue>) -> bool {
    container_rules.as_ref().map(|rule| rule.is_default).unwrap_or(false)
        || field_rule.as_ref().map(|rule| rule.is_default).unwrap_or(false)
}

/// Find `#[deprecated]` attribute from given attributes. Typically derive type attributes
/// or field attributes of struct.
pub(crate) fn get_deprecated(attributes: &[Attribute]) -> Option<Deprecated> {
    attributes.iter().find_map(|attribute| {
        if attribute
            .path()
            .get_ident()
            .map(|ident| *ident == "deprecated")
            .unwrap_or(false)
        {
            Some(Deprecated::True)
        } else {
            None
        }
    })
}

/// Check whether field is required based on following rules.
///
/// * If field has not serde's `skip_serializing_if`
/// * Field has not `serde_with` double option
/// * Field is not default
pub fn is_required(field_rule: Option<&SerdeValue>, container_rules: Option<&SerdeContainer>) -> bool {
    !field_rule.map(|rule| rule.skip_serializing_if).unwrap_or(false)
        && !field_rule.map(|rule| rule.double_option).unwrap_or(false)
        && !is_default(&container_rules, &field_rule)
}

#[derive(Debug)]
enum TypeTreeValue<'t> {
    TypePath(&'t TypePath),
    Path(&'t Path),
    /// Slice and array types need to be manually defined, since they cannot be recognized from
    /// generic arguments.
    Array(Vec<TypeTreeValue<'t>>, Span),
    UnitType,
}

impl PartialEq for TypeTreeValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Path(_) => self == other,
            Self::TypePath(_) => self == other,
            Self::Array(array, _) => matches!(other, Self::Array(other, _) if other == array),
            Self::UnitType => self == other,
        }
    }
}

/// [`TypeTree`] of items which represents a single parsed `type` of a
/// `Schema`, `Parameter` or `FnArg`
#[derive(Clone, Debug, PartialEq)]
pub struct TypeTree<'t> {
    pub path: Option<Cow<'t, Path>>,
    pub value_type: ValueType,
    pub generic_type: Option<GenericType>,
    pub children: Option<Vec<TypeTree<'t>>>,
}

impl<'t> TypeTree<'t> {
    pub fn from_type(ty: &'t Type) -> TypeTree<'t> {
        Self::from_type_paths(Self::get_type_paths(ty))
    }

    fn get_type_paths(ty: &'t Type) -> Vec<TypeTreeValue> {
        match ty {
            Type::Path(path) => {
                vec![TypeTreeValue::TypePath(path)]
            },
            Type::Reference(reference) => Self::get_type_paths(reference.elem.as_ref()),
            Type::Tuple(tuple) => {
                // Detect unit type ()
                if tuple.elems.is_empty() { return vec![TypeTreeValue::UnitType] }

                tuple.elems.iter().flat_map(Self::get_type_paths).collect()
            },
            Type::Group(group) => Self::get_type_paths(group.elem.as_ref()),
            Type::Slice(slice) => vec![TypeTreeValue::Array(Self::get_type_paths(&slice.elem), slice.bracket_token.span.join())],
            Type::Array(array) => vec![TypeTreeValue::Array(Self::get_type_paths(&array.elem), array.bracket_token.span.join())],
            Type::TraitObject(trait_object) => {
                trait_object
                    .bounds
                    .iter()
                    .find_map(|bound| {
                        match &bound {
                            syn::TypeParamBound::Trait(trait_bound) => Some(&trait_bound.path),
                            syn::TypeParamBound::Lifetime(_) => None,
                            syn::TypeParamBound::Verbatim(_) => None,
                            _ => todo!("TypeTree trait object found unrecognized TypeParamBound"),
                        }
                    })
                    .map(|path| vec![TypeTreeValue::Path(path)]).unwrap_or_else(Vec::new)
            }
            _ => abort_call_site!(
                "unexpected type in component part get type path, expected one of: Path, Tuple, Reference, Group, Array, Slice, TraitObject"
            ),
        }
    }

    fn from_type_paths(paths: Vec<TypeTreeValue<'t>>) -> TypeTree<'t> {
        if paths.len() > 1 {
            TypeTree {
                path: None,
                children: Some(Self::convert_types(paths).collect()),
                generic_type: None,
                value_type: ValueType::Tuple,
            }
        } else {
            Self::convert_types(paths)
                .next()
                .expect("TypeTreeValue from_type_paths expected at least one TypePath")
        }
    }

    fn convert_types(paths: Vec<TypeTreeValue<'t>>) -> impl Iterator<Item = TypeTree<'t>> {
        paths.into_iter().map(|value| {
            let path = match value {
                TypeTreeValue::TypePath(type_path) => &type_path.path,
                TypeTreeValue::Path(path) => path,
                TypeTreeValue::Array(value, span) => {
                    let array: Path = Ident::new("Array", span).into();
                    return TypeTree {
                        path: Some(Cow::Owned(array)),
                        value_type: ValueType::Object,
                        generic_type: Some(GenericType::Vec),
                        children: Some(vec![Self::from_type_paths(value)]),
                    };
                }
                TypeTreeValue::UnitType => {
                    return TypeTree {
                        path: None,
                        value_type: ValueType::Tuple,
                        generic_type: None,
                        children: None,
                    }
                }
            };

            // there will always be one segment at least
            let last_segment = path
                .segments
                .last()
                .expect("at least one segment within path in TypeTree::convert_types");

            if last_segment.arguments.is_empty() {
                Self::convert(path, last_segment)
            } else {
                Self::resolve_schema_type(path, last_segment)
            }
        })
    }

    // Only when type is a generic type we get to this function.
    fn resolve_schema_type(path: &'t Path, last_segment: &'t PathSegment) -> TypeTree<'t> {
        if last_segment.arguments.is_empty() {
            abort!(
                last_segment.ident,
                "expected at least one angle bracket argument but was 0"
            );
        };

        let mut generic_schema_type = Self::convert(path, last_segment);

        let mut generic_types = match &last_segment.arguments {
            PathArguments::AngleBracketed(angle_bracketed_args) => {
                // if all type arguments are lifetimes we ignore the generic type
                if angle_bracketed_args
                    .args
                    .iter()
                    .all(|arg| matches!(arg, GenericArgument::Lifetime(_)))
                {
                    None
                } else {
                    Some(
                        angle_bracketed_args
                            .args
                            .iter()
                            .filter(|arg| !matches!(arg, GenericArgument::Lifetime(_)))
                            .map(|arg| match arg {
                                GenericArgument::Type(arg) => arg,
                                _ => abort!(arg, "expected generic argument type or generic argument lifetime"),
                            }),
                    )
                }
            }
            _ => abort!(
                last_segment.ident,
                "unexpected path argument, expected angle bracketed path argument"
            ),
        };

        generic_schema_type.children = generic_types
            .as_mut()
            .map(|generic_type| generic_type.map(Self::from_type).collect());

        generic_schema_type
    }

    fn convert(path: &'t Path, last_segment: &'t PathSegment) -> TypeTree<'t> {
        let generic_type = Self::get_generic_type(last_segment);
        let is_primitive = SchemaType(path).is_primitive();

        Self {
            path: Some(Cow::Borrowed(path)),
            value_type: if is_primitive {
                ValueType::Primitive
            } else {
                ValueType::Object
            },
            generic_type,
            children: None,
        }
    }

    // TODO should we recognize unknown generic types with `GenericType::Unknown` instead of `None`?
    fn get_generic_type(segment: &PathSegment) -> Option<GenericType> {
        match &*segment.ident.to_string() {
            "HashMap" | "Map" | "BTreeMap" => Some(GenericType::Map),
            #[cfg(feature = "indexmap")]
            "IndexMap" => Some(GenericType::Map),
            "Vec" => Some(GenericType::Vec),
            #[cfg(feature = "smallvec")]
            "SmallVec" => Some(GenericType::Vec),
            "Option" => Some(GenericType::Option),
            "Cow" => Some(GenericType::Cow),
            "Box" => Some(GenericType::Box),
            "RefCell" => Some(GenericType::RefCell),
            _ => None,
        }
    }

    /// Check whether [`TypeTreeValue`]'s [`syn::TypePath`] or any if it's `children`s [`syn::TypePath`]
    /// is a given type as [`str`].
    pub fn is(&self, s: &str) -> bool {
        let mut is = self
            .path
            .as_ref()
            .map(|path| {
                path.segments
                    .last()
                    .expect("expected at least one segment in TreeTypeValue path")
                    .ident
                    == s
            })
            .unwrap_or(false);

        if let Some(ref children) = self.children {
            is = is || children.iter().any(|child| child.is(s));
        }

        is
    }

    fn find_mut(&mut self, type_tree: &TypeTree) -> Option<&mut Self> {
        let is = self
            .path
            .as_mut()
            .map(|p| matches!(&type_tree.path, Some(path) if path.as_ref() == p.as_ref()))
            .unwrap_or(false);

        if is {
            Some(self)
        } else {
            self.children
                .as_mut()
                .and_then(|children| children.iter_mut().find_map(|child| Self::find_mut(child, type_tree)))
        }
    }

    /// `Object` virtual type is used when generic object is required in OpenAPI spec. Typically used
    /// with `value_type` attribute to hinder the actual type.
    pub fn is_object(&self) -> bool {
        self.is("Object")
    }

    /// Check whether the [`TypeTree`]'s `generic_type` is [`GenericType::Option`]
    pub fn is_option(&self) -> bool {
        matches!(self.generic_type, Some(GenericType::Option))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Primitive,
    Object,
    Tuple,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum GenericType {
    Vec,
    Map,
    Option,
    Cow,
    Box,
    RefCell,
}

pub(crate) trait Rename {
    fn rename(rule: &RenameRule, value: &str) -> String;
}

/// Performs a rename for given `value` based on given rules. If no rules were
/// provided returns [`None`]
///
/// Method accepts 3 arguments.
/// * `value` to rename.
/// * `to` Optional rename to value for fields with _`rename`_ property.
/// * `container_rule` which is used to rename containers with _`rename_all`_ property.
pub(crate) fn rename<'r, R: Rename>(
    value: &'r str,
    to: Option<Cow<'r, str>>,
    container_rule: Option<&'r RenameRule>,
) -> Option<Cow<'r, str>> {
    let rename = to.and_then(|to| if !to.is_empty() { Some(to) } else { None });

    rename.or_else(|| {
        container_rule
            .as_ref()
            .map(|container_rule| Cow::Owned(R::rename(container_rule, value)))
    })
}

/// Can be used to perform rename on container level e.g `struct`, `enum` or `enum` `variant` level.
struct VariantRename;

impl Rename for VariantRename {
    fn rename(rule: &RenameRule, value: &str) -> String {
        rule.rename_variant(value)
    }
}

/// Can be used to perform rename on field level of a container e.g `struct`.
pub(crate) struct FieldRename;

impl Rename for FieldRename {
    fn rename(rule: &RenameRule, value: &str) -> String {
        rule.rename(value)
    }
}

#[derive(Debug)]
pub struct ComponentSchemaProps<'c> {
    pub type_tree: &'c TypeTree<'c>,
    pub features: Option<Vec<Feature>>,
    pub(crate) description: Option<&'c CommentAttributes>,
    pub(crate) deprecated: Option<&'c Deprecated>,
    pub object_name: &'c str,
}

#[derive(Debug)]
pub struct ComponentSchema {
    tokens: TokenStream,
}

impl<'c> ComponentSchema {
    pub fn new(
        ComponentSchemaProps {
            type_tree,
            features,
            description,
            deprecated,
            object_name,
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
            ),
            Some(GenericType::Vec) => ComponentSchema::vec_to_tokens(
                &mut tokens,
                features,
                type_tree,
                object_name,
                description_stream,
                deprecated_stream,
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
                        .expect("CompnentSchema generic container type should have children")
                        .iter()
                        .next()
                        .expect("CompnentSchema generic container type should have 1 child"),
                    features: Some(features),
                    description,
                    deprecated,
                    object_name,
                })
                .to_tokens(&mut tokens);
            }
            Some(GenericType::Cow) | Some(GenericType::Box) | Some(GenericType::RefCell) => {
                ComponentSchema::new(ComponentSchemaProps {
                    type_tree: type_tree
                        .children
                        .as_ref()
                        .expect("CompnentSchema generic container type should have children")
                        .iter()
                        .next()
                        .expect("CompnentSchema generic container type should have 1 child"),
                    features: Some(features),
                    description,
                    deprecated,
                    object_name,
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
    ) {
        let oapi = crate::oapi_crate();
        let example = features.pop_by(|feature| matches!(feature, Feature::Example(_)));
        let additional_properties = pop_feature!(features => Feature::AdditionalProperties(_));
        let nullable = pop_feature!(features => Feature::Nullable(_));

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
                });

                quote! { .additional_properties(#schema_property) }
            });

        tokens.extend(quote! {
            #oapi::oapi::Object::new()
                #additional_properties
                #description_stream
                #deprecated_stream
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
    ) {
        let oapi = crate::oapi_crate();
        let example = pop_feature!(features => Feature::Example(_));
        let xml = features.extract_vec_xml_feature(type_tree);
        let max_items = pop_feature!(features => Feature::MaxItems(_));
        let min_items = pop_feature!(features => Feature::MinItems(_));
        let nullable = pop_feature!(features => Feature::Nullable(_));

        let child = type_tree
            .children
            .as_ref()
            .expect("CompnentSchema Vec should have children")
            .iter()
            .next()
            .expect("CompnentSchema Vec should have 1 child");

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
            });

            quote! {
                #oapi::oapi::schema::Array::new(#component_schema)
            }
        };

        let validate = |feature: &Feature| {
            let type_path = &**type_tree.path.as_ref().unwrap();
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
    ) {
        let nullable = pop_feature!(features => Feature::Nullable(_));
        let oapi = crate::oapi_crate();

        match type_tree.value_type {
            ValueType::Primitive => {
                let type_path = &**type_tree.path.as_ref().unwrap();
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
                    tokens.extend(quote! {
                        #oapi::oapi::Object::new()
                            #description_stream #deprecated_stream #nullable
                    })
                } else {
                    let type_path = &**type_tree.path.as_ref().unwrap();
                    if is_inline {
                        nullable
                            .map(|nullable| {
                                quote_spanned! {type_path.span()=>
                                    #oapi::oapi::schema::AllOf::new()
                                        #nullable
                                        .item(<#type_path as #oapi::oapi::AsSchema>::schema().1)
                                }
                            })
                            .unwrap_or_else(|| {
                                quote_spanned! {type_path.span() =>
                                    <#type_path as #oapi::oapi::AsSchema>::schema().1
                                }
                            })
                            .to_tokens(tokens);
                    } else {
                        let mut name = Cow::Owned(format_path_ref(type_path));
                        if name == "Self" && !object_name.is_empty() {
                            name = Cow::Borrowed(object_name);
                        }
                        nullable
                            .map(|nullable| {
                                quote! {
                                    #oapi::oapi::schema::AllOf::new()
                                        #nullable
                                        .item(#oapi::oapi::Ref::from_schema_name(#name))
                                }
                            })
                            .unwrap_or_else(|| {
                                quote! {
                                    #oapi::oapi::Ref::from_schema_name(#name)
                                }
                            })
                            .to_tokens(tokens);
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

    fn get_description(comments: Option<&'c CommentAttributes>) -> Option<TokenStream> {
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

    fn get_deprecated(deprecated: Option<&'c Deprecated>) -> Option<TokenStream> {
        deprecated.map(|deprecated| quote! { .deprecated(#deprecated) })
    }
}

impl ToTokens for ComponentSchema {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens)
    }
}
