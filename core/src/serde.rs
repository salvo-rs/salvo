use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
use std::iter::Iterator;
use std::marker::PhantomData;

use multimap::MultiMap;
use serde::de;
use serde_json::value::RawValue;

use crate::extract::metadata::{Source, SourceFormat, SourceFrom};
use crate::http::form::FormData;
use crate::http::ParseError;
use crate::Request;

pub(crate) use serde::de::value::{Error as ValError, MapDeserializer, SeqDeserializer};
use serde::de::{
    Deserialize, DeserializeSeed, Deserializer, EnumAccess, Error as DeError, IntoDeserializer, VariantAccess, Visitor,
};
use serde::forward_to_deserialize_any;

use crate::extract::Metadata;

pub(crate) fn from_str_map<'de, I, T, K, V>(input: I) -> Result<T, ValError>
where
    I: IntoIterator<Item = (K, V)> + 'de,
    T: Deserialize<'de>,
    K: Into<Cow<'de, str>>,
    V: Into<Cow<'de, str>>,
{
    let iter = input.into_iter().map(|(k, v)| (CowValue(k.into()), CowValue(v.into())));
    T::deserialize(MapDeserializer::new(iter))
}

pub(crate) fn from_str_multi_map<'de, I, T, K, C, V>(input: I) -> Result<T, ValError>
where
    I: IntoIterator<Item = (K, C)> + 'de,
    T: Deserialize<'de>,
    K: Into<Cow<'de, str>> + Hash + std::cmp::Eq + 'de,
    C: IntoIterator<Item = V> + 'de,
    V: Into<Cow<'de, str>> + std::cmp::Eq + 'de,
{
    let iter = input
        .into_iter()
        .map(|(k, v)| (CowValue(k.into()), VecValue(v.into_iter().map(|v| CowValue(v.into())))));
    T::deserialize(MapDeserializer::new(iter))
}

pub(crate) async fn from_request<'de, T>(req: &'de mut Request, metadata: &'de Metadata) -> Result<T, ParseError>
where
    T: Deserialize<'de>,
{
    // Ensure body is parsed correctly.
    req.form_data().await.ok();
    req.payload().await.ok();
    Ok(T::deserialize(RequestDeserializer::new(req, metadata)?)?)
}

macro_rules! forward_cow_parsed_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where V: Visitor<'de>
            {
                match self.0.parse::<$ty>() {
                    Ok(val) => val.into_deserializer().$method(visitor),
                    Err(e) => Err(DeError::custom(e))
                }
            }
        )*
    }
}

macro_rules! forward_vec_parsed_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where V: Visitor<'de>
            {
                if let Some(item) = self.0.into_iter().next() {
                    match item.0.parse::<$ty>() {
                        Ok(val) => val.into_deserializer().$method(visitor),
                        Err(e) => Err(DeError::custom(e))
                    }
                } else {
                    Err(DeError::custom("expected vec not empty"))
                }
            }
        )*
    }
}

struct ValueEnumAccess<'de>(Cow<'de, str>);

impl<'de> EnumAccess<'de> for ValueEnumAccess<'de> {
    type Error = ValError;
    type Variant = UnitOnlyVariantAccess;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.0.into_deserializer())?;
        Ok((variant, UnitOnlyVariantAccess))
    }
}

struct UnitOnlyVariantAccess;

impl<'de> VariantAccess<'de> for UnitOnlyVariantAccess {
    type Error = ValError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RequestDeserializer<'de> {
    params: &'de HashMap<String, String>,
    queries: &'de MultiMap<String, String>,
    headers: MultiMap<&'de str, &'de str>,
    form_data: Option<&'de FormData>,
    json_body: Option<HashMap<&'de str, &'de str>>,
    metadata: &'de Metadata,
    field_index: usize,
    field_source: Option<&'de Source>,
    field_value: Option<&'de str>,
}

impl<'de> RequestDeserializer<'de> {
    /// Construct a new `RequestDeserializer<I, E>`.
    pub(crate) fn new(
        request: &'de mut Request,
        metadata: &'de Metadata,
    ) -> Result<RequestDeserializer<'de>, ParseError> {
        let json_body = if let Some(payload) = request.payload.get() {
            Some(
                serde_json::from_slice::<HashMap<&str, &RawValue>>(payload)
                    .map_err(ParseError::SerdeJson)?
                    .into_iter()
                    .map(|(key, value)| (key, value.get()))
                    .collect::<HashMap<&str, &str>>(),
            )
        } else {
            None
        };
        Ok(RequestDeserializer {
            json_body,
            form_data: request.form_data.get(),
            params: request.params(),
            queries: request.queries(),
            headers: request
                .headers()
                .iter()
                .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default()))
                .collect::<MultiMap<_, _>>(),
            metadata,
            field_index: 0,
            field_source: None,
            field_value: None,
        })
    }
    fn deserialize_value<T>(&mut self, seed: T) -> Result<T::Value, ValError>
    where
        T: de::DeserializeSeed<'de>,
    {
        // Panic because this indicates a bug in the program rather than an
        // expected failure.
        let value = self.field_value.expect("MapAccess::next_value called before next_key");
        let source = self
            .field_source
            .take()
            .expect("MapAccess::next_value called before next_key");
        let field = &self.metadata.fields[self.field_index];
        if source.from == SourceFrom::Body && source.format == SourceFormat::Json {
            let mut value = serde_json::Deserializer::new(serde_json::de::StrRead::new(value));
            seed.deserialize(&mut value)
                .map_err(|_| ValError::custom("parse value error"))
        } else {
            seed.deserialize(value.into_deserializer())
        }
    }
    fn next_pair(&mut self) -> Option<(&str, &str)> {
        if self.field_index < self.metadata.fields.len() - 1 {
            let field = &self.metadata.fields[self.field_index];
            let sources = if !field.sources.is_empty() {
                &field.sources
            } else if !self.metadata.default_sources.is_empty() {
                &self.metadata.default_sources
            } else {
                tracing::error!("no sources for field {}", field.name);
                return None;
            };
            for source in sources {
                match source.from {
                    SourceFrom::Param => {
                        if let Some(value) = self.params.get(field.name) {
                            self.field_value = Some(value);
                            self.field_source = Some(source);
                            return Some((field.name, value));
                        }
                    }
                    SourceFrom::Query => {
                        if let Some(value) = self.queries.get(field.name) {
                            self.field_value = Some(value);
                            self.field_source = Some(source);
                            return Some((field.name, value));
                        }
                    }
                    SourceFrom::Header => {
                        if let Some(value) = self.headers.get(field.name) {
                            self.field_value = Some(value);
                            self.field_source = Some(source);
                            return Some((field.name, value));
                        }
                    }
                    SourceFrom::Body => match source.format {
                        SourceFormat::Json => {
                            if let Some(json_body) = &self.json_body {
                                let value = json_body.get(field.name).unwrap();
                                self.field_value = Some(value);
                                self.field_source = Some(source);
                                return Some((field.name, value));
                            } else {
                                return None;
                            }
                        }
                        SourceFormat::MultiMap => {
                            if let Some(form_data) = self.form_data {
                                let value = form_data.fields.get(field.name).unwrap();
                                self.field_value = Some(value);
                                self.field_source = Some(source);
                                return Some((field.name, value));
                            } else {
                                return None;
                            }
                        }
                        _ => {
                            panic!("Unsupported source format: {:?}", source.format);
                        }
                    },
                    _ => {
                        panic!("Unsupported source format: {:?}", source.format);
                    }
                }
            }
            self.field_index += 1;
        }
        None
    }
}
impl<'de> de::Deserializer<'de> for RequestDeserializer<'de> {
    type Error = ValError;

    fn deserialize_any<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_map(&mut self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct tuple_struct map seq tuple
        struct enum identifier ignored_any
    }
}

impl<'de> de::MapAccess<'de> for RequestDeserializer<'de> {
    type Error = ValError;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        match self.next_pair() {
            Some((key, value)) => seed.deserialize(key.into_deserializer()).map(Some),
            None => Ok(None),
        }
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        self.deserialize_value(seed)
    }

    fn next_entry_seed<TK, TV>(&mut self, kseed: TK, vseed: TV) -> Result<Option<(TK::Value, TV::Value)>, Self::Error>
    where
        TK: de::DeserializeSeed<'de>,
        TV: de::DeserializeSeed<'de>,
    {
        match self.next_pair() {
            Some((key, value)) => {
                let key = kseed.deserialize(key.into_deserializer())?;
                let value = self.deserialize_value(vseed)?;
                Ok(Some((key, value)))
            }
            None => Ok(None),
        }
    }
}

struct CowValue<'de>(Cow<'de, str>);
impl<'de> IntoDeserializer<'de> for CowValue<'de> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> Deserializer<'de> for CowValue<'de> {
    type Error = ValError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.0 {
            Cow::Borrowed(value) => visitor.visit_borrowed_str(value),
            Cow::Owned(value) => visitor.visit_string(value),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(ValueEnumAccess(self.0))
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    forward_to_deserialize_any! {
        char
        str
        string
        unit
        bytes
        byte_buf
        unit_struct
        tuple_struct
        struct
        identifier
        tuple
        ignored_any
        seq
        map
    }

    forward_cow_parsed_value! {
        bool => deserialize_bool,
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }
}

struct VecValue<I>(I);
impl<'de, I> IntoDeserializer<'de> for VecValue<I>
where
    I: Iterator<Item = CowValue<'de>>,
{
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de, I> Deserializer<'de> for VecValue<I>
where
    I: IntoIterator<Item = CowValue<'de>>,
{
    type Error = ValError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(item) = self.0.into_iter().next() {
            item.deserialize_any(visitor)
        } else {
            Err(DeError::custom("expected vec not empty"))
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(item) = self.0.into_iter().next() {
            visitor.visit_enum(ValueEnumAccess(item.0.clone()))
        } else {
            Err(DeError::custom("expected vec not empty"))
        }
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_tuple_struct<V>(self, _name: &'static str, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(SeqDeserializer::new(self.0.into_iter()))
    }

    forward_to_deserialize_any! {
        char
        str
        string
        unit
        bytes
        byte_buf
        unit_struct
        struct
        identifier
        ignored_any
        map
    }

    forward_vec_parsed_value! {
        bool => deserialize_bool,
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use multimap::MultiMap;
    use serde::Deserialize;

    use crate::test::TestClient;
    use crate::macros::Extractible;

    #[tokio::test]
    async fn test_de_str_map() {
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct User {
            name: String,
            age: u8,
        }

        let mut data: HashMap<String, String> = HashMap::new();
        data.insert("age".into(), "10".into());
        data.insert("name".into(), "hello".into());
        let user: User = super::from_str_map(&data).unwrap();
        assert_eq!(user.age, 10);
    }

    #[tokio::test]
    async fn test_de_str_multi_map() {
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct User<'a> {
            id: i64,
            name: &'a str,
            age: u8,
            friends: (String, String, i64),
            kids: Vec<String>,
            lala: Vec<i64>,
        }

        let mut map = MultiMap::new();

        map.insert("id", "42");
        map.insert("name", "Jobs");
        map.insert("age", "100");
        map.insert("friends", "100");
        map.insert("friends", "200");
        map.insert("friends", "300");
        map.insert("kids", "aaa");
        map.insert("kids", "bbb");
        map.insert("kids", "ccc");
        map.insert("lala", "600");
        map.insert("lala", "700");

        let user: User = super::from_str_multi_map(map).unwrap();
        assert_eq!(user.id, 42);
    }

    #[tokio::test]
    async fn test_de_request() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[extract(default_source(from = "body", format = "json"))]
        struct RequestData<'a> {
            #[extract(source(from = "param"))]
            param1: i64,
            #[extract(source(from = "param"))]
            param2: &'a str,
            #[extract(source(from = "query"))]
            q1: &'a str,
            #[extract(source(from = "query"))]
            q2: usize,
            #[extract(source(from = "body", format = "json"))]
            body: RequestBody<'a>,
        }
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct RequestBody<'a> {
            title: &'a str,
            content: String,
            comment: &'a str,
            viewers: usize,
        }


        let mut req = TestClient::get("http://127.0.0.1:7878/test/param1v/param2v").query("q1", "q1v").query("q2", "q2v").build();    
        let data: RequestData<'_> = RequestData::extract(&mut req).unwrap();
        assert_eq!(data.param1, "param1");
    }
}
