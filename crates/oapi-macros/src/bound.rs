use std::collections::HashSet;

use syn::Token;
use syn::punctuated::{Pair, Punctuated};

use crate::feature::{Feature, IsSkipped};
use crate::schema::feature::{
    EnumNamedFieldVariantFeatures, EnumUnnamedFieldVariantFeatures, FromAttributes,
    NamedFieldFeatures, NamedFieldStructFeatures, UnnamedFieldStructFeatures,
};
use crate::{DiagResult, Diagnostic, IntoInner, serde_util};

/// Bound inference follows the types used by generated schemas, rather than
/// types replaced or omitted by field attributes. Keep the original AST intact
/// for schema generation and for diagnostics.
pub(crate) fn effective_schema_data(
    data: &syn::Data,
    attributes: &[syn::Attribute],
) -> DiagResult<syn::Data> {
    fn unnamed_override(fields: &mut syn::Fields, features: &[Feature]) -> DiagResult<()> {
        if let syn::Fields::Unnamed(unnamed) = fields
            && unnamed.unnamed.len() != 1
        {
            unnamed.unnamed = unnamed
                .unnamed
                .iter()
                .map(|field| {
                    let rules = serde_util::parse_value(&field.attrs).map_err(Diagnostic::from)?;
                    Ok((!rules.is_some_and(|rules| rules.skip)).then(|| field.clone()))
                })
                .collect::<DiagResult<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
        }
        let Some(Feature::ValueType(value_type)) = features
            .iter()
            .find(|feature| matches!(feature, Feature::ValueType(_)))
        else {
            return Ok(());
        };
        let types = fields
            .iter()
            .map(|field| crate::type_tree::TypeTree::from_type(&field.ty))
            .collect::<DiagResult<Vec<_>>>()?;
        // UnnamedStructSchema applies container overrides to newtypes and
        // homogeneous tuples; heterogeneous tuples keep each original type.
        if types
            .first()
            .is_some_and(|first| types.iter().all(|ty| ty == first))
        {
            for field in fields.iter_mut() {
                field.ty = value_type.0.clone();
            }
        }
        Ok(())
    }
    fn fields(fields: &mut syn::Fields) -> DiagResult<()> {
        if !matches!(fields, syn::Fields::Named(_)) {
            return Ok(());
        }
        for field in fields.iter_mut() {
            let rules = serde_util::parse_value(&field.attrs).map_err(Diagnostic::from)?;
            let features = field
                .attrs
                .parse_features::<NamedFieldFeatures>()?
                .into_inner()
                .unwrap_or_default();
            if rules.is_some_and(|rules| rules.skip)
                || features.is_skipped()
                || features.iter().any(|feature| matches!(feature, Feature::SchemaWith(_)))
                || features.iter().any(|feature| matches!(feature, Feature::Ignore(crate::feature::attributes::Ignore(crate::parse_utils::LitBoolOrExprPath::LitBool(value))) if value.value()))
            {
                field.ty = syn::parse_quote!(());
            } else if let Some(Feature::ValueType(value_type)) = features.iter().find(|feature| matches!(feature, Feature::ValueType(_))) {
                field.ty = value_type.0.clone();
            }
        }
        Ok(())
    }
    let mut data = data.clone();
    if let syn::Data::Struct(data) = &mut data
        && serde_util::parse_container(attributes)
            .map_err(Diagnostic::from)?
            .is_some_and(|rules| rules.transparent)
    {
        let fields = match &data.fields {
            syn::Fields::Named(fields) => &fields.named,
            syn::Fields::Unnamed(fields) => &fields.unnamed,
            syn::Fields::Unit => {
                return Err(Diagnostic::new(
                    crate::DiagLevel::Error,
                    "serde transparent requires a field",
                ));
            }
        };
        let (_, selected) = crate::schema::transparent_field(fields)?;
        let mut field = selected.clone();
        let container_features = match &data.fields {
            syn::Fields::Named(_) => attributes
                .parse_features::<NamedFieldStructFeatures>()?
                .into_inner(),
            _ => attributes
                .parse_features::<UnnamedFieldStructFeatures>()?
                .into_inner(),
        }
        .unwrap_or_default();
        let features = field
            .attrs
            .parse_features::<NamedFieldFeatures>()?
            .into_inner()
            .unwrap_or_default();
        if features
            .iter()
            .any(|feature| matches!(feature, Feature::SchemaWith(_)))
        {
            field.ty = syn::parse_quote!(());
        } else if let Some(Feature::ValueType(value_type)) = features
            .iter()
            .chain(container_features.iter())
            .find(|feature| matches!(feature, Feature::ValueType(_)))
        {
            field.ty = value_type.0.clone();
        }
        field.ident = None;
        field.colon_token = None;
        data.fields = syn::Fields::Unnamed(syn::FieldsUnnamed {
            paren_token: Default::default(),
            unnamed: std::iter::once(field).collect(),
        });
        // The selected value's effective schema type is now complete. Do not
        // reapply tuple-container overrides over its field-level overrides.
        return Ok(syn::Data::Struct(data.clone()));
    }
    match &mut data {
        syn::Data::Struct(data) => {
            if matches!(&data.fields, syn::Fields::Unnamed(_)) {
                let features = attributes
                    .parse_features::<UnnamedFieldStructFeatures>()?
                    .into_inner()
                    .unwrap_or_default();
                unnamed_override(&mut data.fields, &features)?;
            } else {
                fields(&mut data.fields)?;
            }
        }
        syn::Data::Enum(data) => {
            for variant in &mut data.variants {
                if crate::schema::is_skipped_newtype_variant(variant)? {
                    variant.fields = syn::Fields::Unit;
                }
                let features = match &variant.fields {
                    syn::Fields::Named(_) => variant
                        .attrs
                        .parse_features::<EnumNamedFieldVariantFeatures>()?
                        .into_inner()
                        .unwrap_or_default(),
                    syn::Fields::Unnamed(_) => variant
                        .attrs
                        .parse_features::<EnumUnnamedFieldVariantFeatures>()?
                        .into_inner()
                        .unwrap_or_default(),
                    syn::Fields::Unit => Vec::new(),
                };
                if features.is_skipped()
                    || serde_util::parse_value(&variant.attrs)
                        .map_err(Diagnostic::from)?
                        .is_some_and(|rules| rules.skip)
                {
                    variant.fields = syn::Fields::Unit;
                } else {
                    unnamed_override(&mut variant.fields, &features)?;
                    fields(&mut variant.fields)?;
                }
            }
        }
        _ => {}
    }
    Ok(data)
}

// Remove the default from every type parameter because in the generated impls
// they look like associated types: "error: associated type bindings are not
// allowed here".
pub(crate) fn without_defaults(generics: &syn::Generics) -> syn::Generics {
    syn::Generics {
        params: generics
            .params
            .iter()
            .map(|param| match param {
                syn::GenericParam::Type(param) => syn::GenericParam::Type(syn::TypeParam {
                    default: None,
                    ..param.clone()
                }),
                _ => param.clone(),
            })
            .collect(),
        ..generics.clone()
    }
}

pub(crate) fn with_where_predicates(
    generics: &syn::Generics,
    predicates: &[syn::WherePredicate],
) -> syn::Generics {
    let mut generics = generics.clone();
    generics
        .make_where_clause()
        .predicates
        .extend(predicates.iter().cloned());
    generics
}

fn ungroup(mut ty: &syn::Type) -> &syn::Type {
    while let syn::Type::Group(group) = ty {
        ty = &group.elem;
    }
    ty
}

// Puts the given bound on any generic type parameters that are used in fields
// for which filter returns true.
//
// For example, the following struct needs the bound `A: Serialize, B:
// Serialize`.
//
//     struct S<'b, A, B: 'b, C> {
//         a: A,
//         b: Option<&'b B>
//         #[serde(skip_serializing)]
//         c: C,
//     }
pub(crate) fn relevant_types(data: &syn::Data, generics: &syn::Generics) -> Vec<syn::TypePath> {
    struct FindTyParams<'ast> {
        // Set of all generic type parameters on the current struct (A, B, C in
        // the example). Initialized up front.
        all_type_params: HashSet<syn::Ident>,

        // Set of generic type parameters used in fields for which filter
        // returns true (A and B in the example). Filled in as the visitor sees
        // them.
        relevant_type_params: HashSet<syn::Ident>,

        // Fields whose type is an associated type of one of the generic type
        // parameters.
        associated_type_usage: Vec<&'ast syn::TypePath>,
    }

    impl<'ast> FindTyParams<'ast> {
        fn visit_field(&mut self, field: &'ast syn::Field) {
            if let syn::Type::Path(ty) = ungroup(&field.ty)
                && let Some(Pair::Punctuated(t, _)) = ty.path.segments.pairs().next()
                && self.all_type_params.contains(&t.ident)
            {
                self.associated_type_usage.push(ty);
            }
            self.visit_type(&field.ty);
        }

        fn visit_path(&mut self, path: &'ast syn::Path) {
            if let Some(seg) = path.segments.last()
                && seg.ident == "PhantomData"
            {
                // Hardcoded exception, because PhantomData<T> implements
                // Serialize and Deserialize whether or not T implements it.
                return;
            }
            if path.leading_colon.is_none() && path.segments.len() == 1 {
                let id = &path.segments[0].ident;
                if self.all_type_params.contains(id) {
                    self.relevant_type_params.insert(id.clone());
                }
            }
            for segment in &path.segments {
                self.visit_path_segment(segment);
            }
        }

        // Everything below is simply traversing the syntax tree.
        fn visit_type(&mut self, ty: &'ast syn::Type) {
            match ty {
                syn::Type::Array(ty) => self.visit_type(&ty.elem),
                syn::Type::FnPtr(ty) => {
                    for arg in &ty.inputs {
                        self.visit_type(&arg.ty);
                    }
                    self.visit_return_type(&ty.output);
                }
                syn::Type::Group(ty) => self.visit_type(&ty.elem),
                syn::Type::ImplTrait(ty) => {
                    for bound in &ty.bounds {
                        self.visit_type_param_bound(bound);
                    }
                }
                // syn::Type::Macro(ty) => self.visit_macro(&ty.mac),
                syn::Type::Paren(ty) => self.visit_type(&ty.elem),
                syn::Type::Path(ty) => {
                    if let Some(qself) = &ty.qself {
                        self.visit_type(&qself.ty);
                    }
                    self.visit_path(&ty.path);
                }
                syn::Type::Ptr(ty) => self.visit_type(&ty.elem),
                syn::Type::Reference(ty) => self.visit_type(&ty.elem),
                syn::Type::Slice(ty) => self.visit_type(&ty.elem),
                syn::Type::TraitObject(ty) => {
                    for bound in &ty.bounds {
                        self.visit_type_param_bound(bound);
                    }
                }
                syn::Type::Tuple(ty) => {
                    for elem in &ty.elems {
                        self.visit_type(elem);
                    }
                }
                // syn::Type::Infer(_) | syn::Type::Never(_) | syn::Type::Verbatim(_) => {}
                _ => {}
            }
        }

        fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
            self.visit_path_arguments(&segment.arguments);
        }

        fn visit_path_arguments(&mut self, arguments: &'ast syn::PathArguments) {
            match arguments {
                syn::PathArguments::None => {}
                syn::PathArguments::AngleBracketed(arguments) => {
                    for arg in &arguments.args {
                        match arg {
                            syn::GenericArgument::Type(arg) => self.visit_type(arg),
                            syn::GenericArgument::AssocType(arg) => self.visit_type(&arg.ty),
                            syn::GenericArgument::Lifetime(_)
                            | syn::GenericArgument::Const(_)
                            | syn::GenericArgument::AssocConst(_)
                            | syn::GenericArgument::Constraint(_) => {}
                            _ => {}
                        }
                    }
                }
                syn::PathArguments::Parenthesized(arguments) => {
                    for argument in &arguments.inputs {
                        self.visit_type(&argument.ty);
                    }
                    self.visit_return_type(&arguments.output);
                }
            }
        }

        fn visit_return_type(&mut self, return_type: &'ast syn::ReturnType) {
            match return_type {
                syn::ReturnType::Default => {}
                syn::ReturnType::Type(_, output) => self.visit_type(output),
            }
        }

        fn visit_type_param_bound(&mut self, bound: &'ast syn::TypeParamBound) {
            match bound {
                syn::TypeParamBound::Trait(bound) => self.visit_path(&bound.path),
                syn::TypeParamBound::Lifetime(_) | syn::TypeParamBound::Verbatim(_) => {}
                _ => {}
            }
        }

        // Type parameter should not be considered used by a macro path.
        //
        //     struct TypeMacro<T> {
        //         mac: T!(),
        //         marker: PhantomData<T>,
        //     }
        // fn visit_macro(&self, _mac: &'ast syn::Macro) {}
    }

    let all_type_params = generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect();

    let mut visitor = FindTyParams {
        all_type_params,
        relevant_type_params: HashSet::new(),
        associated_type_usage: Vec::new(),
    };
    match data {
        syn::Data::Enum(data) => {
            for variant in data.variants.iter() {
                for field in variant.fields.iter() {
                    visitor.visit_field(field);
                }
            }
        }
        syn::Data::Struct(data) => {
            for field in data.fields.iter() {
                visitor.visit_field(field);
            }
        }
        _ => {}
    }

    let relevant_type_params = visitor.relevant_type_params;
    let associated_type_usage = visitor.associated_type_usage;
    generics
        .type_params()
        .map(|param| param.ident.clone())
        .filter(|id| relevant_type_params.contains(id))
        .map(|id| syn::TypePath {
            attrs: Vec::new(),
            qself: None,
            path: id.into(),
        })
        .chain(associated_type_usage.into_iter().cloned())
        .collect()
}

pub(crate) fn with_bound(
    data: &syn::Data,
    generics: &syn::Generics,
    bounds: &Punctuated<syn::TypeParamBound, syn::token::Plus>,
) -> syn::Generics {
    let new_predicates = relevant_types(data, generics)
        .into_iter()
        .map(|bounded_ty| {
            syn::WherePredicate::Type(syn::PredicateType {
                attrs: Vec::new(),
                lifetimes: None,
                bounded_ty: syn::Type::Path(bounded_ty),
                colon_token: <Token![:]>::default(),
                bounds: bounds.clone(),
            })
        });

    let mut generics = generics.clone();
    generics
        .make_where_clause()
        .predicates
        .extend(new_predicates);
    generics
}
