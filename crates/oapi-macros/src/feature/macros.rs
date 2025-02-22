use crate::IntoInner;
use crate::feature::attributes::*;
use crate::feature::validation::*;
use crate::feature::{Feature, Validatable};

macro_rules! impl_get_name {
    ( $ident:ident = $name:literal ) => {
        impl crate::feature::GetName for $ident {
            fn get_name() -> &'static str {
                $name
            }
        }

        impl Display for $ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let name = <Self as crate::feature::GetName>::get_name();
                write!(f, "{name}")
            }
        }
    };
}

pub(crate) use impl_get_name;

macro_rules! is_validatable {
    ( $( $ident:ident => $validatable:literal ),* $(,)?) => {
        $(
            impl Validatable for $ident {
                fn is_validatable(&self) -> bool {
                    $validatable
                }
            }
        )*
    };
}

is_validatable! {
    Default => false,
    Example => false,
    Examples => false,
    XmlAttr => false,
    Format => false,
    WriteOnly => false,
    ReadOnly => false,
    Name => false,
    Title => false,
    Aliases => false,
    Nullable => false,
    Rename => false,
    DefaultStyle => false,
    Style => false,
    DefaultParameterIn => false,
    ParameterIn => false,
    AllowReserved => false,
    Explode => false,
    RenameAll => false,
    ValueType => false,
    Inline => false,
    ToParametersNames => false,
    MultipleOf => true,
    Maximum => true,
    Minimum => true,
    ExclusiveMaximum => true,
    ExclusiveMinimum => true,
    MaxLength => true,
    MinLength => true,
    Pattern => true,
    MaxItems => true,
    MinItems => true,
    MaxProperties => false,
    MinProperties => false,
    SchemaWith => false,
    Description => false,
    Deprecated => false,
    Skip => false,
    AdditionalProperties => false,
    Required => false,
    SkipBound => false,
    Bound => false,
    ContentEncoding => false,
    ContentMediaType => false
}

macro_rules! parse_features {
    ($ident:ident as $( $feature:path ),* $(,)?) => {
        {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Vec<crate::feature::Feature>> {
                let names = [$( <crate::feature::parse_features!(@as_ident $feature) as crate::feature::GetName>::get_name(), )* ];
                let mut features = Vec::<crate::feature::Feature>::new();
                let attributes = names.join(", ");

                while !input.is_empty() {
                    let ident = input.parse::<syn::Ident>().map_err(|error| {
                        syn::Error::new(
                            error.span(),
                            format!("unexpected attribute, expected any of: {attributes}, {error}"),
                        )
                    })?;
                    let name = &*ident.to_string();

                    $(
                        if name == <crate::feature::parse_features!(@as_ident $feature) as crate::feature::GetName>::get_name() {
                            features.push(<$feature as crate::feature::Parse>::parse(input, ident)?.into());
                            if !input.is_empty() {
                                input.parse::<syn::Token![,]>()?;
                            }
                            continue;
                        }
                    )*

                    if !names.contains(&name) {
                        return Err(syn::Error::new(ident.span(), format!("unexpected attribute: {name}, expected any of: {attributes}")))
                    }
                }

                Ok(features)
            }

            parse($ident)?
        }
    };
    (@as_ident $( $tt:tt )* ) => {
        $( $tt )*
    }
}

pub(crate) use parse_features;

macro_rules! pop_feature {
    ($features:expr => $value:pat_param) => {{ $features.pop_by(|feature| matches!(feature, $value)) }};
}

pub(crate) use pop_feature;

macro_rules! pop_feature_as_inner {
    ( $features:expr => $($value:tt)* ) => {
        crate::feature::pop_feature!($features => $( $value )* )
            .map(|f| match f {
                $( $value )* => {
                    crate::feature::pop_feature_as_inner!( @as_value $( $value )* )
                },
                _ => unreachable!()
            })
    };
    ( @as_value $tt:tt :: $tr:tt ( $v:tt ) ) => {
        $v
    }
}

pub(crate) use pop_feature_as_inner;

macro_rules! impl_feature_into_inner {
    ( $( $feat:ident , )* ) => {
        $(
            impl IntoInner<Option<$feat>> for Option<Feature> {
                fn into_inner(self) -> Option<$feat> {
                    self.and_then(|feature| match feature {
                        Feature::$feat(value) => Some(value),
                        _ => None,
                    })
                }
            }
        )*
    };
}

impl_feature_into_inner! {
    Example,
    Examples,
    Default,
    Inline,
    XmlAttr,
    Format,
    ValueType,
    WriteOnly,
    ReadOnly,
    Title,
    Nullable,
    Rename,
    RenameAll,
    Style,
    AllowReserved,
    Explode,
    ParameterIn,
    ToParametersNames,
    MultipleOf,
    Maximum,
    Minimum,
    ExclusiveMaximum,
    ExclusiveMinimum,
    MaxLength,
    MinLength,
    Pattern,
    MaxItems,
    MinItems,
    MaxProperties,
    MinProperties,
    SchemaWith,
    Description,
    Deprecated,
    Name,
    AdditionalProperties,
    Required,
}

macro_rules! impl_into_inner {
    ($ident:ident) => {
        impl crate::IntoInner<Vec<Feature>> for $ident {
            fn into_inner(self) -> Vec<Feature> {
                self.0
            }
        }

        impl crate::IntoInner<Option<Vec<Feature>>> for Option<$ident> {
            fn into_inner(self) -> Option<Vec<Feature>> {
                self.map(crate::IntoInner::into_inner)
            }
        }
    };
}

pub(crate) use impl_into_inner;

#[allow(dead_code)]
pub(crate) trait Merge<T>: IntoInner<Vec<Feature>> {
    fn merge(self, from: T) -> Self;
}

macro_rules! impl_merge {
    ( $($ident:ident),* ) => {
        $(
            impl AsMut<Vec<Feature>> for $ident {
                fn as_mut(&mut self) -> &mut Vec<Feature> {
                    &mut self.0
                }
            }

            impl crate::feature::Merge<$ident> for $ident {
                fn merge(mut self, from: $ident) -> Self {
                    let a = self.as_mut();
                    let mut b = from.into_inner();

                    a.append(&mut b);

                    self
                }
            }
        )*
    };
}

pub(crate) use impl_merge;
