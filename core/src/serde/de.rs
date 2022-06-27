use std::ops::{AddAssign, MulAssign, Neg};

use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};

#[derive(Debug, Clone)]
pub struct RequestDeserializer<'de>
{
    metadata: &'de Metadata,
    request: &'de Request,
    payload: &'de str,
    field_index: usize,
    field_source: Option<&'a Source>,
    field_value: Option<&'a str>,
}

impl<'de> RequestDeserializer<'de, I, E>
where
    I: Iterator,
    I::Item: private::Pair,
{
    /// Construct a new `RequestDeserializer<I, E>`.
    pub fn new(request: &de Request, metadata: &'de Metadata) -> Self {
        RequestDeserializer {
            request,
            metadata,
        }
    }
    fn  de_value(&self, value: &str) -> Result<Value, E> {
        let field = &de.metadata.fields[self.field_index];
        match f
    }
    fn  next_pair(&mut self) -> Option<(&'a str, &'a str)> {
        if self.field_index < de.metadata.fields.len() - 1 {
            Some(kv) => {
                self.field_index += 1;
                let field = &de.metadata.fields[self.field_index];
                for source in &field.sources {
                    let value = match &*source.name {
                        "params" => {
                            if let Some(value) = req.params().get(field.name) {
                                self.field_source = Some(source);
                                return  Some((field.name, value));
                            }
                        }
                        "queries" => {
                            if let Some(value) = value = req.queries().get(field.name) {
                                all_data.insert(field.name, value);
                            }
                        }
                        "headers" => {
                            let value = req.http_header(field.name).await?;
                            all_data.insert(field.name, value);
                        }
                        "body_form" => {
                            let value = req.http_header(field.name).await?;
                            all_data.insert(field.name, value);
                        }
                        "body_json" => {
                            let value = req.form_data(field.name).await?;
                            all_data.insert(field.name, value);
                        }
    
                        if let Some(value) = source.get(req, &field.name) {
                            all_data.insert(&field.name, value);
                            break;
                        }
                    }
                    all_data.insert(field.name, FieldValue {
                        value, 
                        format: soruce.format,
                    });
                }
               
            }
            None => None,
        }
    }
}
impl <'de, I, E> de::Deserializer<'de> for RequestDeserializer<'de, I, E>
where
    E: de::Error,
{
    type Error = E;

    fn deserialize_any<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let value = try!(visitor.visit_map(&mut self));
        Ok(value)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct tuple_struct map
        struct enum identifier ignored_any
    }
}

impl<'de, I, E> de::MapAccess<'de> for RequestAccess<'de, I, E>
where
    E: de::Error,
{
    type Error = E;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        match self.next_pair() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(key.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self.value.take();
        // Panic because this indicates a bug in the program rather than an
        // expected failure.
        let value = value.expect("MapAccess::next_value called before next_key");
        seed.deserialize(value.into_deserializer())
    }

    fn next_entry_seed<TK, TV>(
        &mut self,
        kseed: TK,
        vseed: TV,
    ) -> Result<Option<(TK::Value, TV::Value)>, Self::Error>
    where
        TK: de::DeserializeSeed<'de>,
        TV: de::DeserializeSeed<'de>,
    {
        match self.next_pair() {
            Some((key, value)) => {
                let key = try!(kseed.deserialize(key.into_deserializer()));
                let value = try!(vseed.deserialize(value.into_deserializer()));
                Ok(Some((key, value)))
            }
            None => Ok(None),
        }
    }
}

impl<'de, I, E> de::SeqAccess<'de> for RequestDeserializer<'de, I, E>
where
    I: Iterator,
    I::Item: private::Pair,
    First<I::Item>: IntoDeserializer<'de, E>,
    Second<I::Item>: IntoDeserializer<'de, E>,
    E: de::Error,
{
    type Error = E;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        match self.next_pair() {
            Some((k, v)) => {
                let de = PairDeserializer(k, v, PhantomData);
                seed.deserialize(de).map(Some)
            }
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        size_hint::from_bounds(&self.iter)
    }
}