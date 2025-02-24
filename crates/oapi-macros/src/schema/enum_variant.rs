use std::borrow::Cow;
use std::marker::PhantomData;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::parse_quote;

use crate::feature::Feature;
use crate::schema_type::SchemaType;
use crate::{Array, DiagResult, TryToTokens};

pub(crate) trait Variant {
    /// Implement `ToTokens` conversion for the [`Variant`]
    fn to_tokens(&self) -> TokenStream;

    /// Get enum variant type. By default enum variant is `string`
    fn get_type(&self) -> (TokenStream, TokenStream) {
        let schema_type = SchemaType {
            path: &parse_quote!(str),
            nullable: false,
        };
        (
            match schema_type.try_to_token_stream() {
                Ok(tokens) => tokens,
                Err(diag) => diag.emit_as_item_tokens(),
            },
            quote! {&str},
        )
    }
}

pub(crate) struct SimpleEnumVariant<T: ToTokens> {
    pub(crate) value: T,
}

impl<T> Variant for SimpleEnumVariant<T>
where
    T: ToTokens,
{
    fn to_tokens(&self) -> TokenStream {
        self.value.to_token_stream()
    }
}

cfg_feature! {
    #![feature ="repr"]
    pub(crate) struct ReprVariant<'r, T: ToTokens> {
        pub(crate) value: T,
        pub(crate) type_path: &'r syn::TypePath,
    }

    impl<T> Variant for ReprVariant<'_, T>
    where
        T: ToTokens,
    {
        fn to_tokens(&self) -> TokenStream {
            self.value.to_token_stream()
        }

        fn get_type(&self) -> (TokenStream, TokenStream) {
        let schema_type = SchemaType {
                    path: &self.type_path.path,
                    nullable: false,
                };
            (
                match schema_type.try_to_token_stream() {
                    Ok(stream) => stream,
                    Err(diag) => diag.emit_as_item_tokens(),
                },
                self.type_path.to_token_stream(),
            )
        }
    }
}

pub(crate) struct ObjectVariant<'o, T: ToTokens> {
    pub(crate) item: T,
    pub(crate) name: Cow<'o, str>,
    pub(crate) title: Option<TokenStream>,
    pub(crate) example: Option<TokenStream>,
}

impl<T> Variant for ObjectVariant<'_, T>
where
    T: ToTokens,
{
    fn to_tokens(&self) -> TokenStream {
        let oapi = crate::oapi_crate();
        let name = &self.name;
        let title = &self.title;
        let example = &self.example;
        let variant = &self.item;

        quote! {
            #oapi::oapi::schema::Object::new()
                #title
                #example
                .property(#name, #variant)
                .required(#name)
        }
    }
}

pub(crate) struct Enum<'e, V: Variant> {
    pub(crate) title: Option<TokenStream>,
    pub(crate) example: Option<TokenStream>,
    len: usize,
    items: Array<'e, TokenStream>,
    schema_type: TokenStream,
    enum_type: TokenStream,
    description: Option<TokenStream>,
    _p: PhantomData<V>,
}

impl<V: Variant> Enum<'_, V> {
    pub(crate) fn new<I: IntoIterator<Item = V>>(items: I) -> Self {
        items.into_iter().collect()
    }

    pub(crate) fn title<I: Into<TokenStream>>(mut self, title: I) -> Self {
        self.title = Some(title.into());
        self
    }

    pub(crate) fn example<I: Into<TokenStream>>(mut self, example: I) -> Self {
        self.example = Some(example.into());
        self
    }

    pub(crate) fn description<I: Into<TokenStream>>(mut self, description: I) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl<T> ToTokens for Enum<'_, T>
where
    T: Variant,
{
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let len = &self.len;
        let title = &self.title;
        let example = &self.example;
        let items = &self.items;
        let schema_type = &self.schema_type;
        let enum_type = &self.enum_type;
        let description = &self.description;

        tokens.extend(quote! {
            #oapi::oapi::Object::new()
                #title
                #description
                #example
                .schema_type(#schema_type)
                .enum_values::<[#enum_type; #len], #enum_type>(#items)
        })
    }
}

impl<V: Variant> FromIterator<V> for Enum<'_, V> {
    fn from_iter<T: IntoIterator<Item = V>>(iter: T) -> Self {
        let mut len = 0;
        let mut schema_type: TokenStream = quote! {};
        let mut enum_type: TokenStream = quote! {};

        let items = iter
            .into_iter()
            .enumerate()
            .map(|(index, variant)| {
                if index == 0 {
                    (schema_type, enum_type) = variant.get_type();
                }
                len = index + 1;
                variant.to_tokens()
            })
            .collect::<Array<TokenStream>>();

        Self {
            title: None,
            example: None,
            description: None,
            len,
            items,
            schema_type,
            enum_type,
            _p: PhantomData,
        }
    }
}

pub(crate) struct TaggedEnum<T: Variant> {
    items: TokenStream,
    len: usize,
    _p: PhantomData<T>,
}

impl<V: Variant> TaggedEnum<V> {
    pub(crate) fn new<'t, I: IntoIterator<Item = (Cow<'t, str>, V)>>(items: I) -> Self {
        items.into_iter().collect()
    }
}

impl<T> ToTokens for TaggedEnum<T>
where
    T: Variant,
{
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let len = &self.len;
        let items = &self.items;

        tokens.extend(quote! {
            Into::<#oapi::oapi::schema::OneOf>::into(#oapi::oapi::schema::OneOf::with_capacity(#len))
                #items
        })
    }
}

impl<'t, V: Variant> FromIterator<(Cow<'t, str>, V)> for TaggedEnum<V> {
    fn from_iter<T: IntoIterator<Item = (Cow<'t, str>, V)>>(iter: T) -> Self {
        let mut len = 0;

        let items = iter
            .into_iter()
            .enumerate()
            .map(|(index, (tag, variant))| {
                let oapi = crate::oapi_crate();
                len = index + 1;

                let (schema_type, enum_type) = variant.get_type();
                let item = variant.to_tokens();
                quote! {
                    .item(
                        #oapi::oapi::schema::Object::new()
                            .property(
                                #tag,
                                #oapi::oapi::schema::Object::new()
                                    .schema_type(#schema_type)
                                    .enum_values::<[#enum_type; 1], #enum_type>([#item])
                            )
                            .required(#tag)
                    )
                }
            })
            .collect::<TokenStream>();

        Self {
            items,
            len,
            _p: PhantomData,
        }
    }
}

pub(crate) struct UntaggedEnum {
    title: Option<Feature>,
}

impl UntaggedEnum {
    pub(crate) fn new() -> Self {
        Self { title: None }
    }

    pub(crate) fn with_title(title: Option<Feature>) -> Self {
        Self { title }
    }
}

impl TryToTokens for UntaggedEnum {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let title = self
            .title
            .as_ref()
            .map(|f| f.try_to_token_stream())
            .transpose()?;

        tokens.extend(quote! {
            #oapi::oapi::schema::Object::new()
                 .schema_type(#oapi::oapi::schema::BasicType::Null)
                .default_value(#oapi::oapi::__private::serde_json::Value::Null)
                #title
        });
        Ok(())
    }
}

pub(crate) struct AdjacentlyTaggedEnum<T: Variant> {
    items: TokenStream,
    len: usize,
    _p: PhantomData<T>,
}

impl<V: Variant> AdjacentlyTaggedEnum<V> {
    pub(crate) fn new<'t, I: IntoIterator<Item = (Cow<'t, str>, Cow<'t, str>, V)>>(
        items: I,
    ) -> Self {
        items.into_iter().collect()
    }
}

impl<T> ToTokens for AdjacentlyTaggedEnum<T>
where
    T: Variant,
{
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let len = &self.len;
        let items = &self.items;

        tokens.extend(quote! {
            Into::<#oapi::oapi::schema::OneOf>::into(#oapi::oapi::schema::OneOf::with_capacity(#len))
                #items
        })
    }
}

impl<'t, V: Variant> FromIterator<(Cow<'t, str>, Cow<'t, str>, V)> for AdjacentlyTaggedEnum<V> {
    fn from_iter<T: IntoIterator<Item = (Cow<'t, str>, Cow<'t, str>, V)>>(iter: T) -> Self {
        let oapi = crate::oapi_crate();
        let mut len = 0;

        let items = iter
            .into_iter()
            .enumerate()
            .map(|(index, (tag, content, variant))| {
                len = index + 1;

                let (schema_type, enum_type) = variant.get_type();
                let item = variant.to_tokens();
                quote! {
                    .item(
                        #oapi::oapi::schema::Object::new()
                            .property(
                                #tag,
                                #oapi::oapi::schema::Object::new()
                                    .schema_type(#oapi::oapi::schema::BasicType::String)
                                    .enum_values::<[#enum_type; 1], #enum_type>([#content])
                            )
                            .required(#tag)
                            .property(
                                #content,
                                #oapi::oapi::schema::Object::new()
                                    .schema_type(#schema_type)
                                    .enum_values::<[#enum_type; 1], #enum_type>([#item])
                            )
                            .required(#content)
                    )
                }
            })
            .collect::<TokenStream>();

        Self {
            items,
            len,
            _p: PhantomData,
        }
    }
}

/// Used to create complex enums with varying Object types.
///
/// Will create `oneOf` object with discriminator field for referenced schemas.
pub(crate) struct CustomEnum<'c, T: ToTokens> {
    // pub(crate) items: Cow<'c, >,
    items: T,
    pub(crate) tag: Option<Cow<'c, str>>,
}

impl<'c, T: ToTokens> CustomEnum<'c, T> {
    pub(crate) fn discriminator(mut self, discriminator: Cow<'c, str>) -> Self {
        self.tag = Some(discriminator);
        self
    }
}

impl<T> ToTokens for CustomEnum<'_, T>
where
    T: ToTokens,
{
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        self.items.to_tokens(tokens);

        // currently uses serde `tag` attribute as a discriminator. This discriminator
        // feature needs some refinement.
        let discriminator = self.tag.as_ref().map(|tag| {
            quote! {
                .discriminator(#oapi::oapi::schema::Discriminator::new(#tag))
            }
        });

        tokens.extend(quote! {
            #discriminator
        });
    }
}

impl FromIterator<TokenStream> for CustomEnum<'_, TokenStream> {
    fn from_iter<T: IntoIterator<Item = TokenStream>>(iter: T) -> Self {
        let oapi = crate::oapi_crate();
        let mut len = 0;

        let items = iter
            .into_iter()
            .enumerate()
            .map(|(index, variant)| {
                len = index + 1;
                quote! {
                    .item(
                        #variant
                    )
                }
            })
            .collect::<TokenStream>();

        let mut tokens = TokenStream::new();

        tokens.extend(quote! {
            Into::<#oapi::oapi::schema::OneOf>::into(#oapi::oapi::schema::OneOf::with_capacity(#len))
                #items
        });

        CustomEnum {
            items: tokens,
            tag: None,
        }
    }
}
