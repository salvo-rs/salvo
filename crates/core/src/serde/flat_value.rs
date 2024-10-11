use std::borrow::Cow;

use serde::de::value::{Error as ValError, SeqDeserializer};
use serde::de::{Deserializer, Error as DeError, IntoDeserializer, Visitor};
use serde::forward_to_deserialize_any;

use super::{CowValue, ValueEnumAccess};

macro_rules! forward_url_query_parsed_value {
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

pub(super) struct FlatValue<'de>(pub(super) Vec<CowValue<'de>>);
impl<'de> IntoDeserializer<'de> for FlatValue<'de> {
    type Deserializer = Self;

    #[inline]
    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> Deserializer<'de> for FlatValue<'de> {
    type Error = ValError;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(item) = self.0.into_iter().next() {
            item.deserialize_any(visitor)
        } else {
            Err(DeError::custom("expected url query not empty"))
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
            visitor.visit_enum(ValueEnumAccess(item.0))
        } else {
            Err(DeError::custom("expected vec not empty"))
        }
    }

    #[inline]
    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    #[inline]
    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
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
    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut items = std::mem::take(&mut self.0);
        let single_mode = if items.len() == 1 {
            if let Some(item) = items.first() {
                item.0.starts_with('[') && item.0.ends_with(']')
            } else {
                false
            }
        } else {
            false
        };
        if single_mode {
            let parser = FlatParser::new(items.remove(0).0);
            visitor.visit_seq(SeqDeserializer::new(parser.into_iter()))
        } else {
            visitor.visit_seq(SeqDeserializer::new(items.into_iter()))
        }
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

    forward_url_query_parsed_value! {
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

struct FlatParser<'de> {
    input: Cow<'de, str>,
    start: usize,
}
impl<'de> FlatParser<'de> {
    fn new(input: Cow<'de, str>) -> Self {
        Self { input, start: 1 }
    }
}
impl<'de> Iterator for FlatParser<'de> {
    type Item = CowValue<'de>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut quote = None;
        let mut in_escape = false;
        let mut end = self.start;
        let mut in_next = false;
        for c in self.input[self.start..].chars() {
            if in_escape {
                in_escape = false;
                continue;
            }
            match c {
                '\\' => {
                    in_escape = true;
                    in_next = true;
                }
                ' ' => {
                    if quote.is_none() {
                        self.start += 1;
                    }
                }
                '"' | '\'' => {
                    in_next = true;
                    if quote == Some(c) {
                        let item = Cow::Owned(self.input[self.start..end].to_string());
                        self.start = end + 2;
                        return Some(CowValue(item));
                    } else {
                        quote = Some(c);
                        self.start += 1;
                    }
                }
                ',' | ']' => {
                    if quote.is_none() && in_next {
                        let item = Cow::Owned(self.input[self.start..end].to_string());
                        self.start = end + 1;
                        return Some(CowValue(item));
                    }
                }
                _ => {
                    in_next = true;
                }
            }
            end += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_flat_parser_1() {
        let parser = super::FlatParser::new("[1,2, 3]".into());
        let mut iter = parser.into_iter();
        assert_eq!(iter.next().unwrap().0, "1");
        assert_eq!(iter.next().unwrap().0, "2");
        assert_eq!(iter.next().unwrap().0, "3");
        assert!(iter.next().is_none());
    }
    #[test]
    fn test_flat_parser_2() {
        let parser = super::FlatParser::new(r#"['3',  '2',"11","1,2"]"#.into());
        let mut iter = parser.into_iter();
        assert_eq!(iter.next().unwrap().0, "3");
        assert_eq!(iter.next().unwrap().0, "2");
        assert_eq!(iter.next().unwrap().0, "11");
        assert_eq!(iter.next().unwrap().0, "1,2");
        assert!(iter.next().is_none());
    }
}
