use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::{Depot, Request, Writer, async_trait};
use serde::{Deserialize, Deserializer};

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Content, Operation, RequestBody, ToRequestBody, ToSchema};

/// Represents the parameters passed by the URI path.
pub struct FormBody<T>(pub T);
impl<T> FormBody<T> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for FormBody<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for FormBody<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de, T> ToRequestBody for FormBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn to_request_body(components: &mut Components) -> RequestBody {
        // Build (and register) the schema once and reuse it for both media types.
        let schema = T::to_schema(components);
        RequestBody::new()
            .description("Extract form format data from request.")
            .add_content(
                "application/x-www-form-urlencoded",
                Content::new(schema.clone()),
            )
            // NOTE: `multipart/*` is not a strictly valid OpenAPI media-type key, but
            // keeping it distinct from `multipart/form-data` avoids clobbering the
            // file schema that `FormFile`/`FormFiles` register under
            // `multipart/form-data` when both are used on the same endpoint. Properly
            // emitting `multipart/form-data` requires merging the form and file
            // schemas; left as follow-up.
            .add_content("multipart/*", Content::new(schema))
    }
}

impl<T> fmt::Debug for FormBody<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Display> Display for FormBody<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'ex, T> Extractible<'ex> for FormBody<T>
where
    T: Deserialize<'ex> + Send,
{
    fn metadata() -> &'static Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(
        req: &'ex mut Request,
        _depot: &'ex mut Depot,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        req.parse_form().await
    }
    async fn extract_with_arg(
        req: &'ex mut Request,
        depot: &'ex mut Depot,
        _arg: &str,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        Self::extract(req, depot).await
    }
}

impl<'de, T> Deserialize<'de> for FormBody<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(FormBody)
    }
}

#[async_trait]
impl<'de, T> EndpointArgRegister for FormBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
        // `to_request_body` already builds and registers the schema; no extra
        // `to_schema` call is needed here.
        let request_body = Self::to_request_body(components);
        operation.request_body = Some(request_body);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use assert_json_diff::assert_json_eq;
    use salvo_core::test::TestClient;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_form_body_into_inner() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(form.into_inner(), "form_body".to_owned());
    }

    #[test]
    fn test_form_body_deref() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(form.deref(), &"form_body".to_owned());
    }

    #[test]
    fn test_form_body_deref_mut() {
        let mut form = FormBody::<String>("form_body".to_owned());
        assert_eq!(form.deref_mut(), &mut "form_body".to_owned());
    }

    #[test]
    fn test_form_body_to_request_body() {
        let mut components = Components::default();
        let request_body = FormBody::<String>::to_request_body(&mut components);
        assert_json_eq!(
            request_body,
            json!({
                "description": "Extract form format data from request.",
                "content": {
                    "application/x-www-form-urlencoded": {
                        "schema": {
                            "type": "string"
                        }
                    },
                    "multipart/*": {
                        "schema": {
                            "type": "string"
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn test_form_body_debug() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(format!("{form:?}"), r#""form_body""#);
    }

    #[test]
    fn test_form_body_display() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(format!("{form}"), "form_body");
    }

    #[test]
    fn test_form_body_metadata() {
        let metadata = FormBody::<String>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    async fn test_form_body_extract_with_arg() {
        let map = BTreeMap::from_iter([("key", "value")]);
        let mut req = TestClient::post("http://127.0.0.1:8698/")
            .form(&map)
            .build();
        let mut depot = Depot::new();
        let result =
            FormBody::<BTreeMap<&str, &str>>::extract_with_arg(&mut req, &mut depot, "key").await;
        assert_eq!("value", result.unwrap().0["key"]);
    }

    #[test]
    fn test_form_body_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        FormBody::<String>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "requestBody": {
                    "content": {
                        "application/x-www-form-urlencoded": {
                            "schema": {
                                "type": "string"
                            }
                        },
                        "multipart/*": {
                            "schema": {
                                "type": "string"
                            }
                        }
                    },
                    "description": "Extract form format data from request."
                },
                "responses": {}
            })
        );
    }
}
