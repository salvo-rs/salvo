use std::borrow::Cow;
use std::hash::Hash;
use std::iter::Iterator;

pub(crate) use serde::de::value::{Error as ValError, MapDeserializer, SeqDeserializer};
use serde::de::{
    Deserialize, DeserializeSeed, Deserializer, EnumAccess, Error as DeError, IntoDeserializer, VariantAccess, Visitor,
};
use serde::forward_to_deserialize_any;

mod request;
pub(crate) use request::from_request;

#[inline]
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

#[inline]
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

#[inline]
pub(crate) fn from_str_multi_val<'de, I, T, C>(input: I) -> Result<T, ValError>
where
    I: IntoIterator<Item = C> + 'de,
    T: Deserialize<'de>,
    C: Into<Cow<'de, str>> + std::cmp::Eq + 'de,
{
    let iter = input.into_iter().map(|v| CowValue(v.into()));
    T::deserialize(VecValue(iter))
}

#[inline]
pub(crate) fn from_str_val<'de, I, T>(input: I) -> Result<T, ValError>
where
    I: Into<Cow<'de, str>>,
    T: Deserialize<'de>,
{
    T::deserialize(CowValue(input.into()))
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

    #[inline]
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

    #[inline]
    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    #[inline]
    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }

    #[inline]
    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }

    #[inline]
    fn struct_variant<V>(self, _fields: &'static [&'static str], _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeError::custom("expected unit variant"))
    }
}

#[derive(Debug)]
struct CowValue<'de>(Cow<'de, str>);
impl<'de> IntoDeserializer<'de> for CowValue<'de> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> Deserializer<'de> for CowValue<'de> {
    type Error = ValError;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.0 {
            Cow::Borrowed(value) => visitor.visit_borrowed_str(value),
            Cow::Owned(value) => visitor.visit_string(value),
        }
    }

    #[inline]
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    #[inline]
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

    #[inline]
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

    #[inline]
    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de, I> Deserializer<'de> for VecValue<I>
where
    I: IntoIterator<Item = CowValue<'de>>,
{
    type Error = ValError;

    #[inline]
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

    #[inline]
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    #[inline]
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

    #[inline]
    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    #[inline]
    fn deserialize_tuple_struct<V>(self, _name: &'static str, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }
    #[inline]
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }
    #[inline]
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
}
