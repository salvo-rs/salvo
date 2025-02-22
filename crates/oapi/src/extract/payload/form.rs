use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::{Request, Writer, async_trait};
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
        RequestBody::new()
            .description("Extract form format data from request.")
            .add_content(
                "application/x-www-form-urlencoded",
                Content::new(T::to_schema(components)),
            )
            .add_content("multipart/*", Content::new(T::to_schema(components)))
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
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(
        req: &'ex mut Request,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        req.parse_form().await
    }
    async fn extract_with_arg(
        req: &'ex mut Request,
        _arg: &str,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        Self::extract(req).await
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
        let request_body = Self::to_request_body(components);
        let _ = <T as ToSchema>::to_schema(components);
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
        let form = FormBody::<String>("form_body".to_string());
        assert_eq!(form.into_inner(), "form_body".to_string());
    }

    #[test]
    fn test_form_body_deref() {
        let form = FormBody::<String>("form_body".to_string());
        assert_eq!(form.deref(), &"form_body".to_string());
    }

    #[test]
    fn test_form_body_deref_mut() {
        let mut form = FormBody::<String>("form_body".to_string());
        assert_eq!(form.deref_mut(), &mut "form_body".to_string());
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
        let form = FormBody::<String>("form_body".to_string());
        assert_eq!(format!("{:?}", form), r#""form_body""#);
    }

    #[test]
    fn test_form_body_display() {
        let form = FormBody::<String>("form_body".to_string());
        assert_eq!(format!("{}", form), "form_body");
    }

    #[test]
    fn test_form_body_metadata() {
        let metadata = FormBody::<String>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    async fn test_form_body_extract_with_arg() {
        let map = BTreeMap::from_iter([("key", "value")]);
        let mut req = TestClient::post("http://127.0.0.1:5800/")
            .form(&map)
            .build();
        let result = FormBody::<BTreeMap<&str, &str>>::extract_with_arg(&mut req, "key").await;
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
