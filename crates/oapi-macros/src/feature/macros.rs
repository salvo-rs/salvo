use crate::feature::{items::*, Feature, Nullable, ParameterIn, Style, Validatable, ValueType};

macro_rules! impl_name {
    ( $ident:ident = $name:literal ) => {
        impl crate::feature::Name for $ident {
            fn get_name() -> &'static str {
                $name
            }
        }

        impl Display for $ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let name = <Self as crate::feature::Name>::get_name();
                write!(f, "{name}")
            }
        }
    };
}

pub(crate) use impl_name;

macro_rules! is_validatable {
    ( $( $ident:ident => $validatable:literal ),* ) => {
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
    XmlAttr => false,
    Format => false,
    WriteOnly => false,
    ReadOnly => false,
    Symbol => false,
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
    Names => false,
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
    AdditionalProperties => false,
    Required => false
}

macro_rules! parse_features {
    ($ident:ident as $( $feature:path ),*) => {
        {
            fn parse(input: syn::parse::ParseStream) -> syn::Result<Vec<crate::feature::Feature>> {
                let names = [$( <crate::feature::parse_features!(@as_ident $feature) as crate::feature::Name>::get_name(), )* ];
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
                        if name == <crate::feature::parse_features!(@as_ident $feature) as crate::feature::Name>::get_name() {
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
    ($features:ident => $value:pat_param) => {{
        $features.pop_by(|feature| matches!(feature, $value))
    }};
}

pub(crate) use pop_feature;

macro_rules! pop_feature_as_inner {
    ( $features:ident => $($value:tt)* ) => {
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

pub(crate) trait IntoInner<T> {
    fn into_inner(self) -> T;
}

macro_rules! impl_into_inner {
    ($ident:ident) => {
        impl crate::feature::IntoInner<Vec<Feature>> for $ident {
            fn into_inner(self) -> Vec<Feature> {
                self.0
            }
        }

        impl crate::feature::IntoInner<Option<Vec<Feature>>> for Option<$ident> {
            fn into_inner(self) -> Option<Vec<Feature>> {
                self.map(crate::feature::IntoInner::into_inner)
            }
        }
    };
}

pub(crate) use impl_into_inner;

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
