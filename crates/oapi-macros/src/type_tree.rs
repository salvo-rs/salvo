use std::borrow::Cow;

use proc_macro2::{Ident, Span};
use syn::spanned::Spanned;
use syn::{GenericArgument, Path, PathArguments, PathSegment, Type, TypePath};

use crate::schema_type::SchemaType;
use crate::{DiagLevel, DiagResult, Diagnostic};

#[derive(Debug)]
enum TypeTreeValue<'t> {
    TypePath(&'t TypePath),
    Path(&'t Path),
    /// Slice and array types need to be manually defined, since they cannot be recognized from
    /// generic arguments.
    Array(Vec<TypeTreeValue<'t>>, Span),
    UnitType,
    Tuple(Vec<TypeTreeValue<'t>>, Span),
}

impl PartialEq for TypeTreeValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Path(_) => self == other,
            Self::TypePath(_) => self == other,
            Self::Array(array, _) => matches!(other, Self::Array(other, _) if other == array),
            Self::Tuple(tuple, _) => matches!(other, Self::Tuple(other, _) if other == tuple),
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
    pub(crate) fn from_type(ty: &'t Type) -> DiagResult<TypeTree<'t>> {
        Self::from_type_paths(Self::get_type_paths(ty)?)
    }

    fn get_type_paths(ty: &Type) -> DiagResult<Vec<TypeTreeValue>> {
        let type_tree_values = match ty {
            Type::Path(path) => {
                vec![TypeTreeValue::TypePath(path)]
            }
            Type::Reference(reference) => Self::get_type_paths(reference.elem.as_ref())?,
            Type::Group(group) => Self::get_type_paths(group.elem.as_ref())?,
            Type::Slice(slice) => vec![TypeTreeValue::Array(
                Self::get_type_paths(&slice.elem)?,
                slice.bracket_token.span.join(),
            )],
            Type::Array(array) => vec![TypeTreeValue::Array(
                Self::get_type_paths(&array.elem)?,
                array.bracket_token.span.join(),
            )],
            Type::Tuple(tuple) => {
                // Detect unit type ()
                if tuple.elems.is_empty() {
                    return Ok(vec![TypeTreeValue::UnitType]);
                }
                vec![TypeTreeValue::Tuple(
                    tuple
                        .elems
                        .iter()
                        .map(Self::get_type_paths)
                        .collect::<DiagResult<Vec<_>>>()?
                        .into_iter()
                        .flatten()
                        .collect(),
                    tuple.span(),
                )]
            }
            Type::TraitObject(trait_object) => trait_object
                .bounds
                .iter()
                .find_map(|bound| match &bound {
                    syn::TypeParamBound::Trait(trait_bound) => Some(&trait_bound.path),
                    syn::TypeParamBound::Lifetime(_) => None,
                    syn::TypeParamBound::Verbatim(_) => None,
                    _ => todo!("TypeTree trait object found unrecognized TypeParamBound"),
                })
                .map(|path| vec![TypeTreeValue::Path(path)])
                .unwrap_or_else(Vec::new),
            unexpected => {
                return Err(Diagnostic::spanned(
                    unexpected.span(),
                    DiagLevel::Error,
                    "unexpected type in component part get type path, expected one of: Path, Tuple, Reference, Group, Array, Slice, TraitObject",
                ));
            }
        };
        Ok(type_tree_values)
    }

    fn from_type_paths(paths: Vec<TypeTreeValue<'t>>) -> DiagResult<TypeTree<'t>> {
        if paths.len() > 1 {
            Ok(TypeTree {
                path: None,
                children: Some(match Self::convert_types(paths) {
                    Ok(children) => children.collect(),
                    Err(diag) => return Err(diag),
                }),
                generic_type: None,
                value_type: ValueType::Tuple,
            })
        } else {
            Ok(Self::convert_types(paths)?
                .next()
                .expect("TypeTreeValue from_type_paths expected at least one TypePath"))
        }
    }

    fn convert_types(
        paths: Vec<TypeTreeValue<'t>>,
    ) -> DiagResult<impl Iterator<Item = TypeTree<'t>>> {
        paths
            .into_iter()
            .map(|value| {
                let path = match value {
                    TypeTreeValue::TypePath(type_path) => &type_path.path,
                    TypeTreeValue::Path(path) => path,
                    TypeTreeValue::Array(value, span) => {
                        let array: Path = Ident::new("Array", span).into();
                        return Ok(TypeTree {
                            path: Some(Cow::Owned(array)),
                            value_type: ValueType::Object,
                            generic_type: Some(GenericType::Vec),
                            children: Some(vec![Self::from_type_paths(value)?]),
                        });
                    }
                    TypeTreeValue::Tuple(tuple, _span) => {
                        return Ok(TypeTree {
                            path: None,
                            generic_type: None,
                            value_type: ValueType::Tuple,
                            children: Some(match Self::convert_types(tuple) {
                                Ok(converted_values) => converted_values.collect(),
                                Err(diag) => return Err(diag),
                            }),
                        });
                    }
                    TypeTreeValue::UnitType => {
                        return Ok(TypeTree {
                            path: None,
                            value_type: ValueType::Tuple,
                            generic_type: None,
                            children: None,
                        });
                    }
                };

                // there will always be one segment at least
                let last_segment = path
                    .segments
                    .last()
                    .expect("at least one segment within path in TypeTree::convert_types");

                if last_segment.arguments.is_empty() {
                    Ok(Self::convert(path, last_segment))
                } else {
                    Self::resolve_schema_type(path, last_segment)
                }
            })
            .collect::<DiagResult<Vec<TypeTree<'t>>>>()
            .map(IntoIterator::into_iter)
    }

    // Only when type is a generic type we get to this function.
    fn resolve_schema_type(
        path: &'t Path,
        last_segment: &'t PathSegment,
    ) -> DiagResult<TypeTree<'t>> {
        if last_segment.arguments.is_empty() {
            return Err(Diagnostic::spanned(
                last_segment.ident.span(),
                DiagLevel::Error,
                "expected at least one angle bracket argument but was 0",
            ));
        };

        let mut generic_schema_type = Self::convert(path, last_segment);

        let mut generic_types = match &last_segment.arguments {
            PathArguments::AngleBracketed(angle_bracketed_args) => {
                // if all type arguments are lifetimes we ignore the generic type
                if angle_bracketed_args.args.iter().all(|arg| {
                    matches!(
                        arg,
                        GenericArgument::Lifetime(_) | GenericArgument::Const(_)
                    )
                }) {
                    None
                } else {
                    Some(
                        angle_bracketed_args
                            .args
                            .iter()
                            .filter(|arg| {
                                !matches!(
                                    arg,
                                    GenericArgument::Lifetime(_) | GenericArgument::Const(_)
                                )
                            })
                            .map(|arg| match arg {
                                GenericArgument::Type(arg) => Ok(arg),
                                _ => Err(Diagnostic::spanned(
                                    arg.span(),
                                    DiagLevel::Error,
                                    "expected generic argument type or generic argument lifetime",
                                )),
                            })
                            .collect::<DiagResult<Vec<_>>>()?
                            .into_iter(),
                    )
                }
            }
            _ => {
                return Err(Diagnostic::spanned(
                    last_segment.ident.span(),
                    DiagLevel::Error,
                    "unexpected path argument, expected angle bracketed path argument",
                ));
            }
        };

        generic_schema_type.children = generic_types
            .as_mut()
            .map(|generic_type| generic_type.map(Self::from_type).collect::<DiagResult<_>>())
            .transpose()?;

        Ok(generic_schema_type)
    }

    fn convert(path: &'t Path, last_segment: &'t PathSegment) -> TypeTree<'t> {
        let generic_type = Self::get_generic_type(last_segment);
        let schema_type = SchemaType {
            path,
            nullable: matches!(generic_type, Some(GenericType::Option)),
        };

        Self {
            path: Some(Cow::Borrowed(path)),
            value_type: if schema_type.is_primitive() {
                ValueType::Primitive
            } else if schema_type.is_value() {
                ValueType::Value
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
            "BTreeSet" | "HashSet" => Some(GenericType::Set),
            "LinkedList" => Some(GenericType::LinkedList),
            #[cfg(feature = "smallvec")]
            "SmallVec" => Some(GenericType::SmallVec),
            "Option" => Some(GenericType::Option),
            "Cow" => Some(GenericType::Cow),
            "Box" => Some(GenericType::Box),
            "Arc" => Some(GenericType::Arc),
            "Rc" => Some(GenericType::Rc),
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

    /// `Value` virtual type is used when any JSON value is required in OpenAPI spec. Typically used
    /// with `value_type` attribute for a member of type `serde_json::Value`.
    pub(crate) fn is_value(&self) -> bool {
        self.is("Value")
    }

    /// Check whether the [`TypeTree`]'s `generic_type` is [`GenericType::Option`]
    pub(crate) fn is_option(&self) -> bool {
        matches!(self.generic_type, Some(GenericType::Option))
    }

    /// Check whether the [`TypeTree`]'s `generic_type` is [`GenericType::Map`]
    pub(crate) fn is_map(&self) -> bool {
        matches!(self.generic_type, Some(GenericType::Map))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueType {
    Primitive,
    Object,
    Tuple,
    Value,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum GenericType {
    Vec,
    LinkedList,
    Set,
    #[cfg(feature = "smallvec")]
    SmallVec,
    Map,
    Option,
    Cow,
    Box,
    RefCell,
    Arc,
    Rc,
}
