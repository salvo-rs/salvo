use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::spanned::Spanned;
use syn::{Error, Ident, LitStr, Path, parse::Parse};

use crate::{DiagLevel, DiagResult, Diagnostic, TryToTokens};

/// Represents data type of [`Schema`].
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum SchemaTypeInner {
    /// Generic schema type allows "properties" with custom types
    Object,
    /// Indicates string type of content.
    String,
    /// Indicates integer type of content.    
    Integer,
    /// Indicates floating point number type of content.
    Number,
    /// Indicates boolean type of content.
    Boolean,
    /// Indicates array type of content.
    Array,
    /// Null type. Used together with other type to indicate nullable values.
    Null,
}

impl ToTokens for SchemaTypeInner {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let ty = match self {
            Self::Object => quote! { #oapi::oapi::schema::BasicType::Object },
            Self::String => quote! { #oapi::oapi::schema::BasicType::String },
            Self::Integer => quote! { #oapi::oapi::schema::BasicType::Integer },
            Self::Number => quote! { #oapi::oapi::schema::BasicType::Number },
            Self::Boolean => quote! { #oapi::oapi::schema::BasicType::Boolean },
            Self::Array => quote! { #oapi::oapi::schema::BasicType::Array },
            Self::Null => quote! { #oapi::oapi::schema::BasicType::Null },
        };
        tokens.extend(ty)
    }
}

/// Tokenizes OpenAPI data type correctly according to the Rust type
pub(crate) struct SchemaType<'a> {
    pub(crate) path: &'a syn::Path,
    pub(crate) nullable: bool,
}

impl SchemaType<'_> {
    fn last_segment_to_string(&self) -> String {
        self.path
            .segments
            .last()
            .expect("Expected at least one segment is_integer")
            .ident
            .to_string()
    }

    pub(crate) fn is_value(&self) -> bool {
        matches!(&*self.last_segment_to_string(), "Value")
    }

    /// Check whether type is known to be primitive in which case returns true.
    pub(crate) fn is_primitive(&self) -> bool {
        let SchemaType { path, .. } = self;
        let last_segment = match path.segments.last() {
            Some(segment) => segment,
            None => return false,
        };
        let name = &*last_segment.ident.to_string();

        #[cfg(not(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time",
            feature = "compact_str"
        )))]
        {
            is_primitive(name)
        }

        #[cfg(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time",
            feature = "compact_str"
        ))]
        {
            let mut primitive = is_primitive(name);

            #[cfg(feature = "chrono")]
            if !primitive {
                primitive = matches!(
                    name,
                    "DateTime" | "NaiveDate" | "Duration" | "NaiveDateTime"
                );
            }
            #[cfg(any(feature = "decimal", feature = "decimal-float"))]
            if !primitive {
                primitive = matches!(name, "Decimal")
            }
            #[cfg(feature = "url")]
            if !primitive {
                primitive = matches!(name, "Url");
            }
            #[cfg(feature = "uuid")]
            if !primitive {
                primitive = matches!(name, "Uuid");
            }
            #[cfg(feature = "ulid")]
            if !primitive {
                primitive = matches!(name, "Ulid");
            }
            #[cfg(feature = "time")]
            if !primitive {
                primitive = matches!(
                    name,
                    "Date" | "PrimitiveDateTime" | "OffsetDateTime" | "Duration"
                );
            }
            #[cfg(feature = "compact_str")]
            if !primitive {
                primitive = matches!(name, "CompactString");
            }

            primitive
        }
    }

    pub(crate) fn is_integer(&self) -> bool {
        matches!(
            &*self.last_segment_to_string(),
            "i8" | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
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
            | "Ipv4Addr"
            | "Ipv6Addr"
    )
}

impl TryToTokens for SchemaType<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let last_segment = self.path.segments.last().ok_or_else(|| {
            Diagnostic::spanned(
                self.path.span(),
                DiagLevel::Error,
                "schema type should have at least one segment in the path",
            )
        })?;
        let name = &*last_segment.ident.to_string();

        fn schema_type_tokens(
            tokens: &mut TokenStream,
            oapi: syn::Ident,
            schema_type: SchemaTypeInner,
            nullable: bool,
        ) {
            if nullable {
                tokens.extend(quote! { #oapi::oapi::schema::SchemaType::from_iter([
                    #schema_type,
                    #oapi::oapi::schema::BasicType::Null
                ])})
            } else {
                tokens.extend(quote! { #oapi::oapi::schema::SchemaType::basic(#schema_type)});
            }
        }

        match name {
            "String" | "str" | "char" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable)
            }
            "bool" => schema_type_tokens(tokens, oapi, SchemaTypeInner::Boolean, self.nullable),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::Integer, self.nullable)
            }
            "f32" | "f64" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::Number, self.nullable)
            }
            #[cfg(feature = "chrono")]
            "DateTime" | "NaiveDateTime" | "NaiveDate" | "NaiveTime" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable)
            }
            #[cfg(any(feature = "chrono", feature = "time"))]
            "Date" | "Duration" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable)
            }
            #[cfg(feature = "compact_str")]
            "CompactString" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable)
            }
            #[cfg(all(feature = "decimal", feature = "decimal-float"))]
            "Decimal" => schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable),
            #[cfg(all(feature = "decimal", not(feature = "decimal-float")))]
            "Decimal" => schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable),
            #[cfg(all(not(feature = "decimal"), feature = "decimal-float"))]
            "Decimal" => schema_type_tokens(tokens, oapi, SchemaTypeInner::Number, self.nullable),
            #[cfg(feature = "url")]
            "Url" => schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable),
            #[cfg(feature = "ulid")]
            "Ulid" => schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable),
            #[cfg(feature = "uuid")]
            "Uuid" => schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable),
            #[cfg(feature = "time")]
            "PrimitiveDateTime" | "OffsetDateTime" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable)
            }
            "Ipv4Addr" | "Ipv6Addr" | "IpAddr" => {
                schema_type_tokens(tokens, oapi, SchemaTypeInner::String, self.nullable)
            }
            _ => schema_type_tokens(tokens, oapi, SchemaTypeInner::Object, self.nullable),
        };
        Ok(())
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

impl TryToTokens for SchemaFormat<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        match self {
            Self::Type(ty) => {
                ty.try_to_tokens(tokens)?;
            }
            Self::Variant(variant) => variant.to_tokens(tokens),
        }
        Ok(())
    }
}

/// Tokenizes OpenAPI data type format correctly by given Rust type.
#[derive(Clone, Debug)]
pub(crate) struct Type<'a>(&'a syn::Path);

impl Type<'_> {
    /// Check is the format know format. Known formats can be used within `quote!{...}` statements.
    pub(crate) fn is_known_format(&self) -> bool {
        let last_segment = match self.0.segments.last() {
            Some(segment) => segment,
            None => return false,
        };
        let name = &*last_segment.ident.to_string();

        #[cfg(not(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time",
            feature = "compact_str"
        )))]
        {
            is_known_format(name)
        }

        #[cfg(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time",
            feature = "compact_str"
        ))]
        {
            let mut known_format = is_known_format(name);

            #[cfg(feature = "chrono")]
            if !known_format {
                known_format = matches!(name, "DateTime" | "NaiveDate" | "NaiveDateTime");
            }
            #[cfg(feature = "decimal")]
            if !known_format {
                known_format = matches!(name, "Decimal");
            }
            #[cfg(feature = "decimal-float")]
            if !known_format {
                known_format = matches!(name, "Decimal");
            }
            #[cfg(feature = "url")]
            if !known_format {
                known_format = matches!(name, "Url");
            }
            #[cfg(feature = "ulid")]
            if !known_format {
                known_format = matches!(name, "Ulid");
            }
            #[cfg(feature = "uuid")]
            if !known_format {
                known_format = matches!(name, "Uuid");
            }

            #[cfg(feature = "time")]
            if !known_format {
                known_format = matches!(name, "Date" | "PrimitiveDateTime" | "OffsetDateTime");
            }

            #[cfg(feature = "compact_str")]
            if !known_format {
                known_format = matches!(name, "CompactString");
            }

            known_format
        }
    }
}

#[inline]
fn is_known_format(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16"
            | "i32"
            | "u8"
            | "u16"
            | "u32"
            | "i64"
            | "u64"
            | "f32"
            | "f64"
            | "Ipv4Addr"
            | "Ipv6Addr"
    )
}

impl TryToTokens for Type<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let last_segment = self.0.segments.last().ok_or_else(|| {
            Diagnostic::spanned(
                self.0.span(),
                DiagLevel::Error,
                "type should have at least one segment in the path",
            )
        })?;
        let name = &*last_segment.ident.to_string();

        match name {
            #[cfg(feature="non-strict-integers")]
            "i8" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int8) }),
            #[cfg(feature="non-strict-integers")]
            "u8" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::UInt8) }),
            #[cfg(feature="non-strict-integers")]
            "i16" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int16) }),
            #[cfg(feature="non-strict-integers")]
            "u16" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::UInt16) }),
            #[cfg(feature="non-strict-integers")]
            #[cfg(feature="non-strict-integers")]
            "u32" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::UInt32) }),
            #[cfg(feature="non-strict-integers")]
            "u64" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::UInt64) }),

            #[cfg(not(feature="non-strict-integers"))]
            "i8" | "i16" | "u8" | "u16" | "u32" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int32) })
            }

            #[cfg(not(feature="non-strict-integers"))]
            "u64" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int64) }),

            "i32" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int32) }),
            "i64" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int64) }),
            "f32" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Float) }),
            "f64" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Double) }),

            #[cfg(any(feature = "decimal", feature = "decimal-float"))]
            "Decimal" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Decimal) })
            }
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
            #[cfg(feature = "compact_str")]
            "CompactString" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::String) })
            }
            #[cfg(feature = "time")]
            "Date" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Date) }),
            #[cfg(feature = "url")]
            "Url" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Url) }),
            #[cfg(feature = "ulid")]
            "Ulid" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Ulid) }),
            #[cfg(feature = "uuid")]
            "Uuid" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Uuid) }),
            #[cfg(feature = "time")]
            "PrimitiveDateTime" | "OffsetDateTime" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::DateTime) })
            },
            "Ipv4Addr" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Ipv4) })
            },
            "Ipv6Addr" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Ipv6) })
            }
            _ => (),
        };

        Ok(())
    }
}

/// [`Parse`] and [`ToTokens`] implementation for [`salvo_oapi::schema::SchemaFormat`].
#[derive(Clone, Debug)]
pub(crate) enum Variant {
    #[cfg(feature = "non-strict-integers")]
    Int8,
    #[cfg(feature = "non-strict-integers")]
    Int16,
    Int32,
    Int64,
    #[cfg(feature = "non-strict-integers")]
    UInt8,
    #[cfg(feature = "non-strict-integers")]
    UInt16,
    #[cfg(feature = "non-strict-integers")]
    UInt32,
    #[cfg(feature = "non-strict-integers")]
    UInt64,
    Float,
    Double,
    Byte,
    Binary,
    Date,
    DateTime,
    Password,
    #[cfg(feature = "url")]
    Url,
    #[cfg(feature = "ulid")]
    Ulid,
    #[cfg(feature = "uuid")]
    Uuid,
    Custom(String),
}

impl Parse for Variant {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let default_formats = [
            "Int32",
            "Int64",
            "Float",
            "Double",
            "Byte",
            "Binary",
            "Date",
            "DateTime",
            "Password",
            #[cfg(feature = "uuid")]
            "Uuid",
            #[cfg(feature = "ulid")]
            "Ulid",
            #[cfg(feature = "url")]
            "Uri",
        ];
        #[cfg(feature = "non-strict-integers")]
        let non_strict_integer_formats = [
            "Int8", "Int16", "Int32", "Int64", "UInt8", "UInt16", "UInt32", "UInt64",
        ];

        #[cfg(feature = "non-strict-integers")]
        let formats = {
            let mut formats = default_formats
                .into_iter()
                .chain(non_strict_integer_formats)
                .collect::<Vec<_>>();
            formats.sort_unstable();
            formats.join(", ")
        };
        #[cfg(not(feature = "non-strict-integers"))]
        let formats = {
            let formats = default_formats.into_iter().collect::<Vec<_>>();
            formats.join(", ")
        };

        let lookahead = input.lookahead1();
        if lookahead.peek(Ident) {
            let format = input.parse::<Ident>()?;
            let name = &*format.to_string();

            match name {
                #[cfg(feature = "non-strict-integers")]
                "Int8" => Ok(Self::Int8),
                #[cfg(feature = "non-strict-integers")]
                "Int16" => Ok(Self::Int16),
                "Int32" => Ok(Self::Int32),
                "Int64" => Ok(Self::Int64),
                #[cfg(feature = "non-strict-integers")]
                "UInt8" => Ok(Self::UInt8),
                #[cfg(feature = "non-strict-integers")]
                "UInt16" => Ok(Self::UInt16),
                #[cfg(feature = "non-strict-integers")]
                "UInt32" => Ok(Self::UInt32),
                #[cfg(feature = "non-strict-integers")]
                "UInt64" => Ok(Self::UInt64),
                "Float" => Ok(Self::Float),
                "Double" => Ok(Self::Double),
                "Byte" => Ok(Self::Byte),
                "Binary" => Ok(Self::Binary),
                "Date" => Ok(Self::Date),
                "DateTime" => Ok(Self::DateTime),
                "Password" => Ok(Self::Password),
                #[cfg(feature = "url")]
                "Url" => Ok(Self::Url),
                #[cfg(feature = "uuid")]
                "Uuid" => Ok(Self::Uuid),
                #[cfg(feature = "ulid")]
                "Ulid" => Ok(Self::Ulid),
                _ => Err(Error::new(
                    format.span(),
                    format!("unexpected format: {name}, expected one of: {formats}"),
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
            #[cfg(feature = "non-strict-integers")]
            Self::Int8 => tokens.extend(quote! {#oapi::oapi::SchemaFormat::KnownFormat(utoipa::openapi::KnownFormat::Int8)}),
            #[cfg(feature = "non-strict-integers")]
            Self::Int16 => tokens.extend(quote! {#oapi::oapi::SchemaFormat::KnownFormat(utoipa::openapi::KnownFormat::Int16)}),

            Self::Int32 => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Int32
            ))),
            Self::Int64 => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Int64
            ))),
            #[cfg(feature = "non-strict-integers")]
            Self::UInt8 => tokens.extend(quote! {#oapi::oapi::SchemaFormat::KnownFormat(utoipa::openapi::KnownFormat::UInt8)}),
            #[cfg(feature = "non-strict-integers")]
            Self::UInt16 => tokens.extend(quote! {#oapi::oapi::SchemaFormat::KnownFormat(utoipa::openapi::KnownFormat::UInt16)}),
            #[cfg(feature = "non-strict-integers")]
            Self::UInt32 => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                utoipa::openapi::KnownFormat::UInt32
            ))),
            #[cfg(feature = "non-strict-integers")]
            Self::UInt64 => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                utoipa::openapi::KnownFormat::UInt64
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
            Self::Ulid => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Ulid
            ))),
            #[cfg(feature = "url")]
            Self::Url => tokens.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Url
            ))),
            Self::Custom(value) => tokens.extend(quote!(#oapi::oapi::SchemaFormat::Custom(
                String::from(#value)
            ))),
        };
    }
}
