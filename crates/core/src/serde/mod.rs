use std::borrow::Cow;
use std::hash::Hash;

pub use serde::de::value::{Error as ValError, MapDeserializer, SeqDeserializer};
use serde::de::{
    Deserialize, DeserializeSeed, EnumAccess, Error as DeError, IntoDeserializer, VariantAccess,
    Visitor,
};

mod request;
pub use request::from_request;
mod cow_value;
use cow_value::CowValue;
mod vec_value;
use vec_value::VecValue;
mod flat_value;
use flat_value::FlatValue;

#[inline]
pub fn from_str_map<'de, I, T, K, V>(input: I) -> Result<T, ValError>
where
    I: IntoIterator<Item = (K, V)> + 'de,
    T: Deserialize<'de>,
    K: Into<Cow<'de, str>>,
    V: Into<Cow<'de, str>>,
{
    let iter = input
        .into_iter()
        .map(|(k, v)| (CowValue(k.into()), CowValue(v.into())));
    T::deserialize(MapDeserializer::new(iter))
}

#[inline]
pub fn from_str_multi_map<'de, I, T, K, C, V>(input: I) -> Result<T, ValError>
where
    I: IntoIterator<Item = (K, C)> + 'de,
    T: Deserialize<'de>,
    K: Into<Cow<'de, str>> + Hash + std::cmp::Eq + 'de,
    C: IntoIterator<Item = V> + 'de,
    V: Into<Cow<'de, str>> + std::cmp::Eq + 'de,
{
    let iter = input.into_iter().map(|(k, v)| {
        (
            CowValue(k.into()),
            VecValue(v.into_iter().map(|v| CowValue(v.into()))),
        )
    });
    T::deserialize(MapDeserializer::new(iter))
}

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
pub fn from_str_val<'de, I, T>(input: I) -> Result<T, ValError>
where
    I: Into<Cow<'de, str>>,
    T: Deserialize<'de>,
{
    T::deserialize(CowValue(input.into()))
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
    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeError::custom("expected unit variant"))
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
