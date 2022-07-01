use std::collections::HashMap;
use std::iter::Iterator;

use multimap::MultiMap;
use serde::de::value::Error as ValError;
use serde::de::{self, Deserialize, Error as DeError, IntoDeserializer, Visitor};
use serde::forward_to_deserialize_any;
use serde_json::value::RawValue;

use crate::extract::metadata::{Source, SourceFormat, SourceFrom};
use crate::extract::Metadata;
use crate::http::form::FormData;
use crate::http::ParseError;
use crate::Request;

use super::CowValue;

pub(crate) async fn from_request<'de, T>(req: &'de mut Request, metadata: &'de Metadata) -> Result<T, ParseError>
where
    T: Deserialize<'de>,
{
    // Ensure body is parsed correctly.
    req.form_data().await.ok();
    req.payload().await.ok();
    Ok(T::deserialize(RequestDeserializer::new(req, metadata)?)?)
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
        let (form_data, json_body) = if let Some(ctype) = request.content_type() {
            if ctype.subtype() == mime::WWW_FORM_URLENCODED || ctype.subtype() == mime::FORM_DATA {
                (request.form_data.get(), None)
            } else if ctype.subtype() == mime::JSON {
                if let Some(payload) = request.payload.get() {
                    let json_body = serde_json::from_slice::<HashMap<&str, &RawValue>>(payload)
                        .map_err(ParseError::SerdeJson)?
                        .into_iter()
                        .map(|(key, value)| (key, value.get()))
                        .collect::<HashMap<&str, &str>>();
                    (None, Some(json_body))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        Ok(RequestDeserializer {
            params: request.params(),
            queries: request.queries(),
            headers: request
                .headers()
                .iter()
                .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default()))
                .collect::<MultiMap<_, _>>(),
            form_data,
            json_body,
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
        // Panic because this indicates a bug in the program rather than an expected failure.
        let value = self.field_value.expect("MapAccess::next_value called before next_key");
        let source = self
            .field_source
            .take()
            .expect("MapAccess::next_value called before next_key");
        if source.from == SourceFrom::Body && source.format == SourceFormat::Json {
            let mut value = serde_json::Deserializer::new(serde_json::de::StrRead::new(value));
            seed.deserialize(&mut value)
                .map_err(|_| ValError::custom("parse value error"))
        } else if source.from == SourceFrom::Request {
            seed.deserialize(RequestDeserializer {
                params: self.params,
                queries: self.queries,
                headers: self.headers.clone(),
                form_data: self.form_data.clone(),
                json_body: self.json_body.clone(),
                metadata: self.metadata,
                field_index: 0,
                field_source: None,
                field_value: None,
            })
        } else {
            seed.deserialize(CowValue(value.into()))
        }
    }
    fn next_pair(&mut self) -> Option<(&str, &str)> {
        if self.field_index < self.metadata.fields.len() {
            let field = &self.metadata.fields[self.field_index];
            let sources = if !field.sources.is_empty() {
                &field.sources
            } else if !self.metadata.default_sources.is_empty() {
                &self.metadata.default_sources
            } else {
                tracing::error!("no sources for field {}", field.name);
                return None;
            };
            self.field_index += 1;
            for source in sources {
                match source.from {
                    SourceFrom::Request => {
                        self.field_value = None;
                        self.field_source = Some(source);
                        return Some((field.name, ""));
                    }
                    SourceFrom::Param => {
                        if let Some(value) = self.params.get(field.name) {
                            self.field_value = Some(value);
                            self.field_source = Some(source);
                            return Some((field.name, value));
                        }
                    }
                    SourceFrom::Query => {
                        if let Some(value) = self.queries.get(field.name) {
                            self.field_value = Some(value.as_str());
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
                    }
                }
            }
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

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct tuple_struct map seq
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
            Some((key, _)) => {
                let key = kseed.deserialize(key.into_deserializer())?;
                let value = self.deserialize_value(vseed)?;
                Ok(Some((key, value)))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use multimap::MultiMap;
    use serde::Deserialize;

    use crate::macros::Extractible;
    use crate::test::TestClient;

    #[tokio::test]
    async fn test_de_request_from_query() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[extract(internal, default_source(from = "query"))]
        struct RequestData {
            q1: String,
            q2: i64,
        }
        let mut req = TestClient::get("http://127.0.0.1:7878/test/1234/param2v")
            .query("q1", "q1v")
            .query("q2", "23")
            .build();
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                q1: "q1v".to_string(),
                q2: 23
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_lifetime() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        # [extract(internal, default_source(from = "query"))]
        struct RequestData<'a> {
            #[extract(source(from = "param"), source(from = "query"))]
            #[extract(source(from = "body"))]
            q1: &'a str,
            // #[extract(source(from = "query"))]
            // #[serde(alias = "param2", alias = "param3")]
            // q2: i64,
        }

        let mut req = TestClient::get("http://127.0.0.1:7878/test/1234/param2v")
            .query("q1", "q1v")
            .query("q2", "23")
            .build();
        let data: RequestData<'_> = req.extract().await.unwrap();
        assert_eq!(data, RequestData { q1: "q1v" });
    }

    #[tokio::test]
    async fn test_de_request_with_mulit_sources() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[extract(internal, default_source(from = "query"))]
        struct RequestData<'a> {
            #[extract(source(from = "param"))]
            #[serde(alias="param1")]
            p1: String,
            #[extract(source(from = "param"))]
            #[serde(alias="param2")]
            p2: &'a str,
            #[extract(source(from = "param"))]
            #[serde(alias="param3")]
            p3: usize,
            // #[extract(source(from = "query"))]
            q1: String,
            // #[extract(source(from = "query"))]
            q2: i64,
            // #[extract(source(from = "body", format = "json"))]
            // body: RequestBody<'a>,
        }

        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct SData {
            #[serde(alias="param1")]
            #[serde(alias="param2")]
            p1: String,
        }

        let d = r#"{"param1":"param1v","params2":"param2v","param3":123,"q1":"q1v","q2":23}"#;
        println!("{:#?}", serde_json::from_str::<SData>(d).unwrap());

        let mut req = TestClient::get("http://127.0.0.1:7878/test/1234/param2v")
            .query("q1", "q1v")
            .query("q2", "23")
            .build();
            req.params.insert("param1".into(), "param1v".into());
            req.params.insert("p2".into(), "921".into());
            req.params.insert("p3".into(), "89785".into());
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(data, RequestData { p1: "param1v".into(), p2: "921", p3: 89785,  q1: "q1v".into(), q2: 23 });
    }
}
