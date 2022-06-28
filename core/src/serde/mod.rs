use std::borrow::Cow;
use std::hash::Hash;
use std::iter::Iterator;

pub(crate) use serde::de::value::{Error, MapDeserializer, SeqDeserializer};
use serde::de::{
    Deserialize, DeserializeSeed, Deserializer, EnumAccess, Error as DeError, IntoDeserializer, VariantAccess, Visitor,
};
use serde::forward_to_deserialize_any;

mod map;
mod request;
pub use map::*;
pub use request::*;

pub(crate) fn from_str_map<'de, I, T, K, V>(input: I) -> Result<T, Error>
where
    I: IntoIterator<Item = (K, V)> + 'de,
    T: Deserialize<'de>,
    K: Into<Cow<'de, str>>,
    V: Into<Cow<'de, str>>,
{
    let iter = input.into_iter().map(|(k, v)| (CowValue(k.into()), CowValue(v.into())));
    T::deserialize(MapDeserializer::new(iter))
}

pub(crate) fn from_str_value_map<'de, I, T, K, C, V>(input: I) -> Result<T, Error>
where
    I: IntoIterator<Item = (K, C)> + 'de,
    T: Deserialize<'de>,
    K: Into<Cow<'de, str>> + Hash + std::cmp::Eq + 'de,
    C: IntoIterator<Item = V> + 'de,
    V: Into<Cow<'de, str>> + std::cmp::Eq + 'de,
{
    let iter = input
        .into_iter()
        .map(|(k, v)| (CowValue(k.into()), FieldValue));
    T::deserialize(MapDeserializer::new(iter))
}

pub(crate) fn from_str_multi_map<'de, I, T, K, C, V>(input: I) -> Result<T, Error>
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

pub(crate) fn from_request<T>(request: &mut Request, metadata: &Metadata) -> Result<T, Error>
where
    T: Deserialize<'de>,
{
    T::deserialize(RequestDeserializer::new(q, metadata))
}