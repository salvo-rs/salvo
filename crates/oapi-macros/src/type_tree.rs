use std::borrow::Cow;

use proc_macro2::{Ident, Span};
use proc_macro_error::{abort, abort_call_site};
use syn::{GenericArgument, Path, PathArguments, PathSegment, Type, TypePath};

use crate::schema_type::SchemaType;

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
pub(crate) struct TypeTree<'t> {
    pub(crate) path: Option<Cow<'t, Path>>,
    pub(crate) value_type: ValueType,
    pub(crate) generic_type: Option<GenericType>,
    pub(crate) children: Option<Vec<TypeTree<'t>>>,
}

impl<'t> TypeTree<'t> {
    pub(crate) fn from_type(ty: &'t Type) -> TypeTree<'t> {
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
    pub(crate) fn is(&self, s: &str) -> bool {
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

    // pub(crate) fn find_mut(&mut self, type_tree: &TypeTree) -> Option<&mut Self> {
    //     let is = self
    //         .path
    //         .as_mut()
    //         .map(|p| matches!(&type_tree.path, Some(path) if path.as_ref() == p.as_ref()))
    //         .unwrap_or(false);

    //     if is {
    //         Some(self)
    //     } else {
    //         self.children
    //             .as_mut()
    //             .and_then(|children| children.iter_mut().find_map(|child| Self::find_mut(child, type_tree)))
    //     }
    // }

    /// `Object` virtual type is used when generic object is required in OpenAPI spec. Typically used
    /// with `value_type` attribute to hinder the actual type.
    pub(crate) fn is_object(&self) -> bool {
        self.is("Object")
    }

    /// Check whether the [`TypeTree`]'s `generic_type` is [`GenericType::Option`]
    pub(crate) fn is_option(&self) -> bool {
        matches!(self.generic_type, Some(GenericType::Option))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueType {
    Primitive,
    Object,
    Tuple,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum GenericType {
    Vec,
    Map,
    Option,
    Cow,
    Box,
    RefCell,
}
