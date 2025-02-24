use std::borrow::Cow;
use std::collections::HashMap;

use indexmap::IndexMap;
use multimap::MultiMap;
use serde::de::value::Error as ValError;
use serde::de::{self, Deserialize, Error as DeError, IntoDeserializer};
use serde::forward_to_deserialize_any;
use serde_json::value::RawValue;

use crate::Request;
use crate::extract::Metadata;
use crate::extract::metadata::{Field, Source, SourceFrom, SourceParser};
use crate::http::ParseError;
use crate::http::form::FormData;
use crate::http::header::HeaderMap;

use super::{CowValue, FlatValue, VecValue};

pub async fn from_request<'de, T>(
    req: &'de mut Request,
    metadata: &'de Metadata,
) -> Result<T, ParseError>
where
    T: Deserialize<'de>,
{
    // Ensure body is parsed correctly.
    if let Some(ctype) = req.content_type() {
        match ctype.subtype() {
            mime::WWW_FORM_URLENCODED | mime::FORM_DATA => {
                if metadata.has_body_required() {
                    let _ = req.form_data().await;
                }
            }
            mime::JSON => {
                if metadata.has_body_required() {
                    let _ = req.payload().await;
                }
            }
            _ => {}
        }
    }
    Ok(T::deserialize(RequestDeserializer::new(req, metadata)?)?)
}

#[derive(Clone, Debug)]
pub(crate) enum Payload<'a> {
    FormData(&'a FormData),
    JsonStr(&'a str),
    JsonMap(HashMap<&'a str, &'a RawValue>),
}
impl Payload<'_> {
    #[allow(dead_code)]
    pub(crate) fn is_form_data(&self) -> bool {
        matches!(*self, Self::FormData(_))
    }
    pub(crate) fn is_json_str(&self) -> bool {
        matches!(*self, Self::JsonStr(_))
    }
    pub(crate) fn is_json_map(&self) -> bool {
        matches!(*self, Self::JsonMap(_))
    }
}

#[derive(Debug)]
pub(crate) struct RequestDeserializer<'de> {
    params: &'de IndexMap<String, String>,
    queries: &'de MultiMap<String, String>,
    #[cfg(feature = "cookie")]
    cookies: &'de cookie::CookieJar,
    headers: &'de HeaderMap,
    payload: Option<Payload<'de>>,
    metadata: &'de Metadata,
    field_index: isize,
    field_flatten: bool,
    field_source: Option<&'de Source>,
    field_str_value: Option<&'de str>,
    field_vec_value: Option<Vec<CowValue<'de>>>,
}

impl<'de> RequestDeserializer<'de> {
    /// Construct a new `RequestDeserializer<I, E>`.
    pub(crate) fn new(
        request: &'de Request,
        metadata: &'de Metadata,
    ) -> Result<RequestDeserializer<'de>, ParseError> {
        let mut payload = None;

        if metadata.has_body_required() {
            if let Some(ctype) = request.content_type() {
                match ctype.subtype() {
                    mime::WWW_FORM_URLENCODED | mime::FORM_DATA => {
                        payload = request.form_data.get().map(Payload::FormData);
                    }
                    mime::JSON => {
                        if let Some(data) = request.payload.get() {
                            if !data.is_empty() {
                                // https://github.com/serde-rs/json/issues/903
                                payload = match serde_json::from_slice::<HashMap<&str, &RawValue>>(
                                    data,
                                ) {
                                    Ok(map) => Some(Payload::JsonMap(map)),
                                    Err(e) => {
                                        tracing::warn!(error = ?e, "`RequestDeserializer` serde parse json payload failed");
                                        Some(Payload::JsonStr(std::str::from_utf8(data)?))
                                    }
                                };
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(RequestDeserializer {
            params: request.params(),
            queries: request.queries(),
            headers: request.headers(),
            #[cfg(feature = "cookie")]
            cookies: request.cookies(),
            payload,
            metadata,
            field_index: -1,
            field_flatten: false,
            field_source: None,
            field_str_value: None,
            field_vec_value: None,
        })
    }

    fn real_parser(&self, source: &Source) -> SourceParser {
        let mut parser = source.parser;
        if parser == SourceParser::Smart {
            if source.from == SourceFrom::Body {
                if let Some(payload) = &self.payload {
                    if payload.is_json_map() || payload.is_json_str() {
                        parser = SourceParser::Json;
                    } else {
                        parser = SourceParser::MultiMap;
                    }
                } else {
                    parser = SourceParser::MultiMap;
                }
            } else if source.from == SourceFrom::Query || source.from == SourceFrom::Header {
                parser = SourceParser::Flat;
            } else {
                parser = SourceParser::MultiMap;
            }
        }
        parser
    }

    fn deserialize_value<T>(&mut self, seed: T) -> Result<T::Value, ValError>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.field_flatten {
            let field = self
                .metadata
                .fields
                .get(self.field_index as usize)
                .expect("field must exist.");
            let metadata = field.metadata.expect("field's metadata must exist");
            seed.deserialize(RequestDeserializer {
                params: self.params,
                queries: self.queries,
                headers: self.headers,
                #[cfg(feature = "cookie")]
                cookies: self.cookies,
                payload: self.payload.clone(),
                metadata,
                field_index: -1,
                field_flatten: false,
                field_source: None,
                field_str_value: None,
                field_vec_value: None,
            })
        } else {
            let source = self
                .field_source
                .take()
                .expect("`MapAccess::next_value` called before next_key");

            let parser = self.real_parser(source);
            if source.from == SourceFrom::Body && parser == SourceParser::Json {
                // panic because this indicates a bug in the program rather than an expected failure.
                let value = self
                    .field_str_value
                    .expect("MapAccess::next_value called before next_key");
                let mut value = serde_json::Deserializer::new(serde_json::de::StrRead::new(value));

                seed.deserialize(&mut value)
                    .map_err(|_| ValError::custom("parse value error"))
            } else if let Some(value) = self.field_str_value.take() {
                seed.deserialize(CowValue(value.into()))
            } else if let Some(value) = self.field_vec_value.take() {
                if source.from == SourceFrom::Query || source.from == SourceFrom::Header {
                    seed.deserialize(FlatValue(value))
                } else {
                    seed.deserialize(VecValue(value.into_iter()))
                }
            } else {
                Err(ValError::custom("parse value error"))
            }
        }
    }

    #[allow(unreachable_patterns)]
    fn fill_value(&mut self, field: &'de Field) -> bool {
        if field.flatten {
            self.field_flatten = true;
            return true;
        }
        let sources = if !field.sources.is_empty() {
            &field.sources
        } else if !self.metadata.default_sources.is_empty() {
            &self.metadata.default_sources
        } else {
            tracing::error!("no sources for field {}", field.decl_name);
            return false;
        };

        let field_name = if let Some(rename) = field.rename {
            rename
        } else if let Some(serde_rename) = field.serde_rename {
            serde_rename
        } else if let Some(rename_all) = self.metadata.rename_all {
            &*rename_all.apply_to_field(field.decl_name)
        } else if let Some(serde_rename_all) = self.metadata.serde_rename_all {
            &*serde_rename_all.apply_to_field(field.decl_name)
        } else {
            field.decl_name
        };

        for source in sources {
            match source.from {
                SourceFrom::Param => {
                    let mut value = self.params.get(field_name);
                    if value.is_none() {
                        for alias in &field.aliases {
                            value = self.params.get(*alias);
                            if value.is_some() {
                                break;
                            }
                        }
                    }
                    if let Some(value) = value {
                        self.field_str_value = Some(value);
                        self.field_source = Some(source);
                        return true;
                    }
                }
                SourceFrom::Query => {
                    let mut value = self.queries.get_vec(field_name);
                    if value.is_none() {
                        for alias in &field.aliases {
                            value = self.queries.get_vec(*alias);
                            if value.is_some() {
                                break;
                            }
                        }
                    }
                    if let Some(value) = value {
                        self.field_vec_value =
                            Some(value.iter().map(|v| CowValue(v.into())).collect());
                        self.field_source = Some(source);
                        return true;
                    }
                }
                SourceFrom::Header => {
                    let mut value = None;
                    if self.headers.contains_key(field_name) {
                        value = Some(self.headers.get_all(field_name))
                    } else {
                        for alias in &field.aliases {
                            if self.headers.contains_key(*alias) {
                                value = Some(self.headers.get_all(*alias));
                                break;
                            }
                        }
                    };
                    if let Some(value) = value {
                        self.field_vec_value = Some(
                            value
                                .iter()
                                .map(|v| CowValue(Cow::from(v.to_str().unwrap_or_default())))
                                .collect(),
                        );
                        self.field_source = Some(source);
                        return true;
                    }
                }
                #[cfg(feature = "cookie")]
                SourceFrom::Cookie => {
                    let mut value = None;
                    if let Some(cookie) = self.cookies.get(field_name.as_ref()) {
                        value = Some(cookie.value());
                    } else {
                        for alias in &field.aliases {
                            if let Some(cookie) = self.cookies.get(alias) {
                                value = Some(cookie.value());
                                break;
                            }
                        }
                    };
                    if let Some(value) = value {
                        self.field_str_value = Some(value);
                        self.field_source = Some(source);
                        return true;
                    }
                }
                SourceFrom::Body => {
                    let parser = self.real_parser(source);
                    match parser {
                        SourceParser::Json => {
                            if let Some(payload) = &self.payload {
                                match payload {
                                    Payload::FormData(form_data) => {
                                        let mut value = form_data.fields.get(field_name);
                                        if value.is_none() {
                                            for alias in &field.aliases {
                                                value = form_data.fields.get(*alias);
                                                if value.is_some() {
                                                    break;
                                                }
                                            }
                                        }
                                        if let Some(value) = value {
                                            self.field_str_value = Some(value);
                                            self.field_source = Some(source);
                                            return true;
                                        }
                                        return false;
                                    }
                                    Payload::JsonMap(map) => {
                                        let mut value = map.get(field_name);
                                        if value.is_none() {
                                            for alias in &field.aliases {
                                                value = map.get(alias);
                                                if value.is_some() {
                                                    break;
                                                }
                                            }
                                        }
                                        if let Some(value) = value {
                                            self.field_str_value = Some(value.get());
                                            self.field_source = Some(source);
                                            return true;
                                        }
                                        return false;
                                    }
                                    Payload::JsonStr(value) => {
                                        self.field_str_value = Some(*value);
                                        self.field_source = Some(source);
                                        return true;
                                    }
                                }
                            } else {
                                return false;
                            }
                        }
                        SourceParser::MultiMap => {
                            if let Some(Payload::FormData(form_data)) = self.payload {
                                let mut value = form_data.fields.get_vec(field_name);
                                if value.is_none() {
                                    for alias in &field.aliases {
                                        value = form_data.fields.get_vec(*alias);
                                        if value.is_some() {
                                            break;
                                        }
                                    }
                                }
                                if let Some(value) = value {
                                    self.field_vec_value = Some(
                                        value.iter().map(|v| CowValue(Cow::from(v))).collect(),
                                    );
                                    self.field_source = Some(source);
                                    return true;
                                }
                            }
                            return false;
                        }
                        _ => {
                            panic!("unsupported source parser: {:?}", parser);
                        }
                    }
                }
            }
        }
        false
    }
    fn next(&mut self) -> Option<Cow<'_, str>> {
        while self.field_index < self.metadata.fields.len() as isize - 1 {
            self.field_index += 1;
            let field = &self.metadata.fields[self.field_index as usize];
            self.field_flatten = field.flatten;
            self.field_str_value = None;
            self.field_vec_value = None;

            if self.fill_value(field) {
                return field.serde_rename.map(Cow::from).or_else(|| {
                    if let Some(serde_rename_all) = self.metadata.serde_rename_all {
                        Some(Cow::Owned(serde_rename_all.apply_to_field(field.decl_name)))
                    } else {
                        Some(Cow::from(field.decl_name))
                    }
                });
            }
        }
        None
    }
}

impl<'de> de::Deserializer<'de> for RequestDeserializer<'de> {
    type Error = ValError;

    #[inline]
    fn deserialize_any<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_map(&mut self)
    }

    #[inline]
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

    #[inline]
    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        match self.next() {
            Some(key) => seed.deserialize(key.into_deserializer()).map(Some),
            None => Ok(None),
        }
    }

    #[inline]
    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        self.deserialize_value(seed)
    }

    #[inline]
    fn next_entry_seed<TK, TV>(
        &mut self,
        kseed: TK,
        vseed: TV,
    ) -> Result<Option<(TK::Value, TV::Value)>, Self::Error>
    where
        TK: de::DeserializeSeed<'de>,
        TV: de::DeserializeSeed<'de>,
    {
        match self.next() {
            Some(key) => {
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
    use serde::{Deserialize, Serialize};

    use crate::macros::Extractible;
    use crate::test::TestClient;

    #[tokio::test]
    async fn test_de_request_from_query() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "query")))]
        struct RequestData {
            q1: String,
            q2: i64,
        }
        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
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
        #[salvo(extract(default_source(from = "query")))]
        struct RequestData<'a> {
            #[salvo(extract(source(from = "param"), source(from = "query")))]
            #[salvo(extract(source(from = "body")))]
            q1: &'a str,
            // #[salvo(extract(source(from = "query")))]
            // #[serde(alias = "param2", alias = "param3")]
            // q2: i64,
        }

        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .query("q1", "q1v")
            .query("q2", "23")
            .build();
        let data: RequestData<'_> = req.extract().await.unwrap();
        assert_eq!(data, RequestData { q1: "q1v" });
    }

    #[tokio::test]
    async fn test_de_request_with_rename() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "query")))]
        struct RequestData<'a> {
            #[salvo(extract(source(from = "param"), source(from = "query"), rename = "abc"))]
            q1: &'a str,
        }

        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .query("abc", "q1v")
            .build();
        let data: RequestData<'_> = req.extract().await.unwrap();
        assert_eq!(data, RequestData { q1: "q1v" });
    }

    #[tokio::test]
    async fn test_de_request_with_rename_all() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "query"), rename_all = "PascalCase"))]
        struct RequestData<'a> {
            first_name: &'a str,
            #[salvo(extract(rename = "lastName"))]
            last_name: &'a str,
        }

        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .query("FirstName", "chris")
            .query("lastName", "young")
            .build();
        let data: RequestData<'_> = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                first_name: "chris",
                last_name: "young"
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_multi_sources() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "query")))]
        struct RequestData<'a> {
            #[salvo(extract(source(from = "param")))]
            #[salvo(extract(alias = "param1"))]
            p1: String,
            #[salvo(extract(source(from = "param"), alias = "param2"))]
            p2: &'a str,
            #[salvo(extract(source(from = "param"), alias = "param3"))]
            p3: usize,
            // #[salvo(extract(source(from = "query")))]
            q1: String,
            // #[salvo(extract(source(from = "query")))]
            q2: i64,
            // #[salvo(extract(source(from = "body", parse = "json")))]
            // body: RequestBody<'a>,
        }

        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .query("q1", "q1v")
            .query("q2", "23")
            .build();
        req.params.insert("param1".into(), "param1v".into());
        req.params.insert("p2".into(), "921".into());
        req.params.insert("p3".into(), "89785".into());
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                p1: "param1v".into(),
                p2: "921",
                p3: 89785,
                q1: "q1v".into(),
                q2: 23
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_json_vec() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "body", parse = "json")))]
        struct RequestData<'a> {
            #[salvo(extract(source(from = "param")))]
            p2: &'a str,
            users: Vec<User>,
        }
        #[derive(Deserialize, Serialize, Eq, PartialEq, Debug)]
        struct User {
            id: i64,
            name: String,
        }

        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .json(&vec![
                User {
                    id: 1,
                    name: "chris".into(),
                },
                User {
                    id: 2,
                    name: "young".into(),
                },
            ])
            .build();
        req.params.insert("p2".into(), "921".into());
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                p2: "921",
                users: vec![
                    User {
                        id: 1,
                        name: "chris".into()
                    },
                    User {
                        id: 2,
                        name: "young".into()
                    }
                ]
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_json_bool() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "body", parse = "json")))]
        struct RequestData<'a> {
            #[salvo(extract(source(from = "param")))]
            p2: &'a str,
            b: bool,
        }

        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .json(&true)
            .build();
        req.params.insert("p2".into(), "921".into());
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(data, RequestData { p2: "921", b: true });
    }

    #[tokio::test]
    async fn test_de_request_with_json_str() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "body", parse = "json")))]
        struct RequestData<'a> {
            #[salvo(extract(source(from = "param")))]
            p2: &'a str,
            s: &'a str,
        }

        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .json(&"abcd-good")
            .build();
        req.params.insert("p2".into(), "921".into());
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                p2: "921",
                s: "abcd-good"
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_form_json_str() {
        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct User<'a> {
            name: &'a str,
            age: usize,
        }
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "body", parse = "json")))]
        struct RequestData<'a> {
            #[salvo(extract(source(from = "param")))]
            p2: &'a str,
            user: User<'a>,
        }
        let mut req = TestClient::get("http://127.0.0.1:5800/test/1234/param2v")
            .raw_form(r#"user={"name": "chris", "age": 20}"#)
            .build();
        req.params.insert("p2".into(), "921".into());
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                p2: "921",
                user: User {
                    name: "chris",
                    age: 20
                }
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_extract_rename_all() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(rename_all = "kebab-case", default_source(from = "query")))]
        struct RequestData {
            full_name: String,
            #[salvo(extract(rename = "currAge"))]
            curr_age: usize,
        }
        let mut req = TestClient::get(
            "http://127.0.0.1:5800/test/1234/param2v?full-name=chris+young&currAge=20",
        )
        .build();
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                full_name: "chris young".into(),
                curr_age: 20
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_serde_rename_all() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "query")))]
        #[serde(rename_all = "kebab-case")]
        struct RequestData {
            full_name: String,
            #[salvo(extract(rename = "currAge"))]
            curr_age: usize,
        }
        let mut req = TestClient::get(
            "http://127.0.0.1:5800/test/1234/param2v?full-name=chris+young&currAge=20",
        )
        .build();
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                full_name: "chris young".into(),
                curr_age: 20
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_with_both_rename_all() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(rename_all = "kebab-case", default_source(from = "query")))]
        #[serde(rename_all = "camelCase")]
        struct RequestData {
            full_name: String,
            #[salvo(extract(rename = "currAge"))]
            curr_age: usize,
        }
        let mut req = TestClient::get(
            "http://127.0.0.1:5800/test/1234/param2v?full-name=chris+young&currAge=20",
        )
        .build();
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                full_name: "chris young".into(),
                curr_age: 20
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_url_array() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "query")))]
        struct RequestData {
            ids: Vec<String>,
        }
        let mut req =
            TestClient::get("http://127.0.0.1:5800/test/1234/param2v?ids=[3,2,11]").build();
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                ids: vec!["3".to_string(), "2".to_string(), "11".to_string()]
            }
        );
        let mut req = TestClient::get(
            r#"http://127.0.0.1:5800/test/1234/param2v?ids=['3',  '2',"11","1,2"]"#,
        )
        .build();
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                ids: vec![
                    "3".to_string(),
                    "2".to_string(),
                    "11".to_string(),
                    "1,2".to_string()
                ]
            }
        );
    }

    #[tokio::test]
    async fn test_de_request_url_array2() {
        #[derive(Deserialize, Extractible, Eq, PartialEq, Debug)]
        #[salvo(extract(default_source(from = "query")))]
        struct RequestData {
            ids: Vec<i64>,
        }
        let mut req =
            TestClient::get("http://127.0.0.1:5800/test/1234/param2v?ids=[3,2,11]").build();
        let data: RequestData = req.extract().await.unwrap();
        assert_eq!(
            data,
            RequestData {
                ids: vec![3, 2, 11]
            }
        );
    }
}
