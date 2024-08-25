use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::{Request, Writer};
use serde::{Deserialize, Deserializer};

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Content, Operation, RequestBody, ToRequestBody, ToSchema};

/// Represents the parameters passed by the URI path.
pub struct JsonBody<T>(pub T);
impl<T> JsonBody<T> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for JsonBody<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for JsonBody<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de, T> ToRequestBody for JsonBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn to_request_body(components: &mut Components) -> RequestBody {
        RequestBody::new()
            .description("Extract json format data from request.")
            .add_content("application/json", Content::new(T::to_schema(components)))
    }
}

impl<T> fmt::Debug for JsonBody<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> fmt::Display for JsonBody<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'ex, T> Extractible<'ex> for JsonBody<T>
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
        req.parse_json().await
    }
    async fn extract_with_arg(
        req: &'ex mut Request,
        _arg: &str,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        Self::extract(req).await
    }
}

impl<'de, T> Deserialize<'de> for JsonBody<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(JsonBody)
    }
}

impl<'de, T> EndpointArgRegister for JsonBody<T>
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
    fn test_json_body_into_inner() {
        let form = JsonBody::<String>("json_body".to_string());
        assert_eq!(form.into_inner(), "json_body".to_string());
    }

    #[test]
    fn test_json_body_deref() {
        let form = JsonBody::<String>("json_body".to_string());
        assert_eq!(form.deref(), &"json_body".to_string());
    }

    #[test]
    fn test_json_body_deref_mut() {
        let mut form = JsonBody::<String>("json_body".to_string());
        assert_eq!(form.deref_mut(), &mut "json_body".to_string());
    }

    #[test]
    fn test_json_body_to_request_body() {
        let mut components = Components::default();
        let request_body = JsonBody::<String>::to_request_body(&mut components);
        assert_json_eq!(
            request_body,
            json!({
                "description": "Extract json format data from request.",
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "string"
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn test_json_body_debug() {
        let form = JsonBody::<String>("json_body".to_string());
        assert_eq!(format!("{:?}", form), r#""json_body""#);
    }

    #[test]
    fn test_json_body_display() {
        let form = JsonBody::<String>("json_body".to_string());
        assert_eq!(format!("{}", form), "json_body");
    }

    #[test]
    fn test_json_body_metadata() {
        let metadata = JsonBody::<String>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    async fn test_json_body_extract_with_arg() {
        let map = BTreeMap::from_iter([("key", "value")]);
        let mut req = TestClient::post("http://127.0.0.1:5800/")
            .json(&map)
            .build();
        let result = JsonBody::<BTreeMap<&str, &str>>::extract_with_arg(&mut req, "key").await;
        assert_eq!("value", result.unwrap().0["key"]);
    }

    #[test]
    fn test_json_body_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        JsonBody::<String>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "requestBody": {
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "string"
                            }
                        }
                    },
                    "description": "Extract json format data from request."
                },
                "responses": {}
            })
        );
    }
}
