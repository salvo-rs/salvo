use proc_macro2::TokenStream;
use proc_macro_error::abort_call_site;
use quote::{quote, ToTokens};
use syn::{parse::Parse, Error, Ident, LitStr, Path};

/// Tokenizes OpenAPI data type correctly according to the Rust type
pub(crate) struct SchemaType<'a>(pub(crate) &'a syn::Path);

impl SchemaType<'_> {
    fn last_segment_to_string(&self) -> String {
        self.0
            .segments
            .last()
            .expect("Expected at least one segment is_integer")
            .ident
            .to_string()
    }

    /// Check whether type is known to be primitive in which case returns true.
    pub(crate) fn is_primitive(&self) -> bool {
        let SchemaType(path) = self;
        let last_segment = match path.segments.last() {
            Some(segment) => segment,
            None => return false,
        };
        let name = &*last_segment.ident.to_string();

        #[cfg(not(any(
            feature = "chrono",
            feature = "decimal",
            feature = "ulid",
            feature = "url",
            feature = "uuid",
            feature = "time",
        )))]
        {
            is_primitive(name)
        }

        #[cfg(any(
            feature = "chrono",
            feature = "decimal",
            feature = "ulid",
            feature = "url",
            feature = "uuid",
            feature = "time",
        ))]
        {
            let mut primitive = is_primitive(name);

            #[cfg(feature = "chrono")]
            if !primitive {
                primitive = is_primitive_chrono(name);
            }

            #[cfg(feature = "decimal")]
            if !primitive {
                primitive = is_primitive_rust_decimal(name);
            }

            #[cfg(feature = "uuid")]
            if !primitive {
                primitive = matches!(name, "Uuid");
            }
            #[cfg(feature = "ulid")]
            if !primitive {
                primitive = matches!(name, "Ulid");
            }
            #[cfg(feature = "url")]
            if !primitive {
                primitive = matches!(name, "Url");
            }

            #[cfg(feature = "time")]
            if !primitive {
                primitive = matches!(name, "Date" | "PrimitiveDateTime" | "OffsetDateTime" | "Duration");
            }

            primitive
        }
    }

    pub(crate) fn is_integer(&self) -> bool {
        matches!(
            &*self.last_segment_to_string(),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
        )
    }

    pub(crate) fn is_unsigned_integer(&self) -> bool {
        matches!(
            &*self.last_segment_to_string(),
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
        )
    }

    pub(crate) fn is_number(&self) -> bool {
        match &*self.last_segment_to_string() {
            "f32" | "f64" => true,
            _ if self.is_integer() => true,
            _ => false,
        }
    }

    pub(crate) fn is_string(&self) -> bool {
        matches!(&*self.last_segment_to_string(), "str" | "String")
    }

    pub(crate) fn is_byte(&self) -> bool {
        matches!(&*self.last_segment_to_string(), "u8")
    }
}

#[inline]
fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "str"
            | "char"
            | "bool"
            | "usize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "isize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "f32"
            | "f64"
    )
}

#[inline]
#[cfg(feature = "chrono")]
fn is_primitive_chrono(name: &str) -> bool {
    matches!(name, "DateTime" | "Date" | "NaiveDate" | "Duration" | "NaiveDateTime")
}

#[inline]
#[cfg(feature = "decimal")]
fn is_primitive_rust_decimal(name: &str) -> bool {
    matches!(name, "Decimal")
}

impl ToTokens for SchemaType<'_> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let oapi = crate::oapi_crate();
        let last_segment = self
            .0
            .segments
            .last()
            .unwrap_or_else(|| abort_call_site!("expected there to be at least one segment in the path"));
        let name = &*last_segment.ident.to_string();

        match name {
            "String" | "str" | "char" => tokens.extend(quote! {#oapi::oapi::SchemaType::String}),
            "bool" => tokens.extend(quote! { #oapi::oapi::SchemaType::Boolean }),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => {
                tokens.extend(quote! { #oapi::oapi::SchemaType::Integer })
            }
            "f32" | "f64" => tokens.extend(quote! { #oapi::oapi::SchemaType::Number }),
            #[cfg(feature = "chrono")]
            "DateTime" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            #[cfg(feature = "chrono")]
            "NaiveDateTime" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            #[cfg(feature = "chrono")]
            "NaiveDate" => tokens.extend(quote!(#oapi::oapi::SchemaType::String)),
            #[cfg(any(feature = "chrono", feature = "time"))]
            "Date" | "Duration" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            #[cfg(feature = "decimal")]
            "Decimal" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            #[cfg(feature = "ulid")]
            "Ulid" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            #[cfg(feature = "uuid")]
            "Uuid" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            #[cfg(feature = "url")]
            "Url" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            #[cfg(feature = "time")]
            "PrimitiveDateTime" | "OffsetDateTime" => tokens.extend(quote! { #oapi::oapi::SchemaType::String }),
            _ => tokens.extend(quote! { #oapi::oapi::SchemaType::Object }),
        }
    }
}

/// Either Rust type component variant or enum variant schema variant.
#[derive(Clone, Debug)]
pub(crate) enum SchemaFormat<'c> {
    /// [`salvo_oapi::schema::SchemaFormat`] enum variant schema format.
    Variant(Variant),
    /// Rust type schema format.
    Type(Type<'c>),
}

impl SchemaFormat<'_> {
    pub(crate) fn is_known_format(&self) -> bool {
        match self {
            Self::Type(ty) => ty.is_known_format(),
            Self::Variant(_) => true,
        }
    }
}

impl<'a> From<&'a Path> for SchemaFormat<'a> {
    fn from(path: &'a Path) -> Self {
        Self::Type(Type(path))
    }
}

impl Parse for SchemaFormat<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self::Variant(input.parse()?))
    }
}

impl ToTokens for SchemaFormat<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Type(ty) => ty.to_tokens(tokens),
            Self::Variant(variant) => variant.to_tokens(tokens),
        }
    }
}

/// Tokenizes OpenAPI data type format correctly by given Rust type.
#[derive(Clone, Debug)]
pub(crate) struct Type<'a>(&'a syn::Path);

impl Type<'_> {
    /// Check is the format know format. Known formats can be used within `quote! {...}` statements.
    pub(crate) fn is_known_format(&self) -> bool {
        let last_segment = match self.0.segments.last() {
            Some(segment) => segment,
            None => return false,
        };
        let name = &*last_segment.ident.to_string();

        #[cfg(not(any(
            feature = "chrono",
            feature = "ulid",
            feature = "url",
            feature = "uuid",
            feature = "time"
        )))]
        {
            is_known_format(name)
        }

        #[cfg(any(
            feature = "chrono",
            feature = "ulid",
            feature = "url",
            feature = "uuid",
            feature = "time"
        ))]
        {
            let mut known_format = is_known_format(name);

            #[cfg(feature = "chrono")]
            if !known_format {
                known_format = matches!(name, "DateTime" | "Date" | "NaiveDate" | "NaiveDateTime");
            }

            #[cfg(feature = "ulid")]
            if !known_format {
                known_format = matches!(name, "Ulid");
            }
            #[cfg(feature = "uuid")]
            if !known_format {
                known_format = matches!(name, "Uuid");
            }
            #[cfg(feature = "url")]
            if !known_format {
                known_format = matches!(name, "Url");
            }

            #[cfg(feature = "time")]
            if !known_format {
                known_format = matches!(name, "Date" | "PrimitiveDateTime" | "OffsetDateTime");
            }

            known_format
        }
    }
}

#[inline]
fn is_known_format(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16" | "i32" | "u8" | "u16" | "u32" | "i64" | "u64" | "f32" | "f64"
    )
}

impl ToTokens for Type<'_> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let oapi = crate::oapi_crate();
        let last_segment = self
            .0
            .segments
            .last()
            .unwrap_or_else(|| abort_call_site!("expected there to be at least one segment in the path"));
        let name = &*last_segment.ident.to_string();

        match name {
            "i8" | "i16" | "i32" | "u8" | "u16" | "u32" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int32) })
            }
            "i64" | "u64" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int64) })
            }
            "f32" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Float) }),
            "f64" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Double) }),
            #[cfg(feature = "chrono")]
            "NaiveDate" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Date) })
            }
            #[cfg(feature = "chrono")]
            "DateTime" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::DateTime) })
            }
            #[cfg(feature = "chrono")]
            "NaiveDateTime" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::DateTime) })
            }
            #[cfg(any(feature = "chrono", feature = "time"))]
            "Date" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Date) }),
            #[cfg(feature = "ulid")]
            "Ulid" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Ulid) }),
            #[cfg(feature = "uuid")]
            "Uuid" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Uuid) }),
            #[cfg(feature = "url")]
            "Url" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Url) }),
            #[cfg(feature = "time")]
            "PrimitiveDateTime" | "OffsetDateTime" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::DateTime) })
            }
            _ => (),
        }
    }
}

/// [`Parse`] and [`ToTokens`] implementation for [`salvo_oapi::schema::SchemaFormat`].
#[derive(Clone, Debug)]
pub(crate) enum Variant {
    Int32,
    Int64,
    Float,
    Double,
    Byte,
    Binary,
    Date,
    DateTime,
    Password,
    #[cfg(feature = "ulid")]
    Ulid,
    #[cfg(feature = "uuid")]
    Uuid,
    #[cfg(feature = "url")]
    Url,
    Custom(String),
}

impl Parse for Variant {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const FORMATS: [&str; 12] = [
            "Int32", "Int64", "Float", "Double", "Byte", "Binary", "Date", "DateTime", "Password", "Ulid", "Uuid",
            "Url",
        ];
        let excluded_format: &[&str] = &[
            #[cfg(not(feature = "uuid"))]
            "Uuid",
            #[cfg(not(feature = "ulid"))]
            "Ulid",
            #[cfg(not(feature = "url"))]
            "Uri",
        ];
        let known_formats = FORMATS
            .into_iter()
            .filter(|format| !excluded_format.contains(format))
            .collect::<Vec<_>>();

        let lookahead = input.lookahead1();
        if lookahead.peek(Ident) {
            let format = input.parse::<Ident>()?;
            let name = &*format.to_string();

            match name {
                "Int32" => Ok(Self::Int32),
                "Int64" => Ok(Self::Int64),
                "Float" => Ok(Self::Float),
                "Double" => Ok(Self::Double),
                "Byte" => Ok(Self::Byte),
                "Binary" => Ok(Self::Binary),
                "Date" => Ok(Self::Date),
                "DateTime" => Ok(Self::DateTime),
                "Password" => Ok(Self::Password),
                #[cfg(feature = "uuid")]
                "Uuid" => Ok(Self::Uuid),
                #[cfg(feature = "ulid")]
                "Ulid" => Ok(Self::Ulid),
                #[cfg(feature = "url")]
                "Url" => Ok(Self::Url),
                _ => Err(Error::new(
                    format.span(),
                    format!(
                        "unexpected format: {name}, expected one of: {}",
                        known_formats.join(", ")
                    ),
                )),
            }
        } else if lookahead.peek(LitStr) {
            let value = input.parse::<LitStr>()?.value();
            Ok(Self::Custom(value))
        } else {
            Err(lookahead.error())
        }
    }
}

impl ToTokens for Variant {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        match self {
            Self::Int32 => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Int32
            ))),
            Self::Int64 => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Int64
            ))),
            Self::Float => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Float
            ))),
            Self::Double => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Double
            ))),
            Self::Byte => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Byte
            ))),
            Self::Binary => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Binary
            ))),
            Self::Date => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Date
            ))),
            Self::DateTime => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::DateTime
            ))),
            Self::Password => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Password
            ))),
            #[cfg(feature = "uuid")]
            Self::Uuid => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Uuid
            ))),
            #[cfg(feature = "ulid")]
            Self::Ulid => tokens.extend(quote!(utoipa::openapi::SchemaFormat::KnownFormat(
                utoipa::openapi::KnownFormat::Ulid
            ))),
            #[cfg(feature = "url")]
            Self::Url => tokens.extend(quote!(utoipa::openapi::SchemaFormat::KnownFormat(
                utoipa::openapi::KnownFormat::Url
            ))),
            Self::Custom(value) => tokens.extend(quote!(#oapi::oapi::SchemaFormat::Custom(
                String::from(#value)
            ))),
        };
    }
}
