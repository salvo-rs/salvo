use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::Request;
use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use serde::{Deserialize, Deserializer};

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Operation, Parameter, ParameterIn, ToSchema};

/// Represents the parameters passed by the URI path.
pub struct QueryParam<T, const REQUIRED: bool = true>(Option<T>);
impl<T> QueryParam<T, true> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0.expect("`QueryParam<T, true>` into_inner get `None`")
    }
}
impl<T> QueryParam<T, false> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> Option<T> {
        self.0
    }
}

impl<T> Deref for QueryParam<T, true> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("`QueryParam<T, true>` defref get `None`")
    }
}
impl<T> Deref for QueryParam<T, false> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for QueryParam<T, true> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
            .as_mut()
            .expect("`QueryParam<T, true>` defref_mut get `None`")
    }
}
impl<T> DerefMut for QueryParam<T, false> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de, T, const R: bool> Deserialize<'de> for QueryParam<T, R>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(|value| QueryParam(Some(value)))
    }
}

impl<T: Debug, const R: bool> Debug for QueryParam<T, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Display> Display for QueryParam<T, true> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0
            .as_ref()
            .expect("`QueryParam<T, true>` as_ref get `None`")
            .fmt(f)
    }
}

impl<'ex, T> Extractible<'ex> for QueryParam<T, true>
where
    T: Deserialize<'ex>,
{
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request) -> Result<Self, ParseError> {
        panic!("query parameter can not be extracted from request")
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(req: &'ex mut Request, arg: &str) -> Result<Self, ParseError> {
        let value = req.query(arg).ok_or_else(|| {
            ParseError::other(format!(
                "query parameter {} not found or convert to type failed",
                arg
            ))
        })?;
        Ok(Self(value))
    }
}
impl<'ex, T> Extractible<'ex> for QueryParam<T, false>
where
    T: Deserialize<'ex>,
{
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request) -> Result<Self, ParseError> {
        panic!("query parameter can not be extracted from request")
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(req: &'ex mut Request, arg: &str) -> Result<Self, ParseError> {
        Ok(Self(req.query(arg)))
    }
}

impl<T, const R: bool> EndpointArgRegister for QueryParam<T, R>
where
    T: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let parameter = Parameter::new(arg)
            .parameter_in(ParameterIn::Query)
            .description(format!("Get parameter `{arg}` from request url query."))
            .schema(T::to_schema(components))
            .required(R);
        operation.parameters.insert(parameter);
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use salvo_core::test::TestClient;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_required_query_param_into_inner() {
        let param = QueryParam::<String, true>(Some("param".to_string()));
        assert_eq!("param".to_string(), param.into_inner());
    }

    #[test]
    fn test_required_query_param_deref() {
        let param = QueryParam::<String, true>(Some("param".to_string()));
        assert_eq!(&"param".to_string(), param.deref())
    }

    #[test]
    fn test_required_query_param_deref_mut() {
        let mut param = QueryParam::<String, true>(Some("param".to_string()));
        assert_eq!(&mut "param".to_string(), param.deref_mut())
    }

    #[test]
    fn test_query_param_into_inner() {
        let param = QueryParam::<String, false>(Some("param".to_string()));
        assert_eq!(Some("param".to_string()), param.into_inner());
    }

    #[test]
    fn test_query_param_deref() {
        let param = QueryParam::<String, false>(Some("param".to_string()));
        assert_eq!(&Some("param".to_string()), param.deref())
    }

    #[test]
    fn test_query_param_deref_mut() {
        let mut param = QueryParam::<String, false>(Some("param".to_string()));
        assert_eq!(&mut Some("param".to_string()), param.deref_mut())
    }

    #[test]
    fn test_query_param_deserialize() {
        let param = serde_json::from_str::<QueryParam<String, true>>(r#""param""#).unwrap();
        assert_eq!(param.0.unwrap(), "param");
    }

    #[test]
    fn test_query_param_debug() {
        let param = QueryParam::<String, true>(Some("param".to_string()));
        assert_eq!(format!("{:?}", param), r#"Some("param")"#);
    }

    #[test]
    fn test_query_param_display() {
        let param = QueryParam::<String, true>(Some("param".to_string()));
        assert_eq!(format!("{}", param), "param");
    }

    #[test]
    fn test_required_query_param_metadata() {
        let metadata = QueryParam::<String, true>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_required_query_prarm_extract() {
        let mut req = Request::new();
        let _ = QueryParam::<String, true>::extract(&mut req).await;
    }

    #[tokio::test]
    async fn test_required_query_prarm_extract_with_value() {
        let req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        req.queries_mut()
            .insert("param".to_string(), "param".to_string());
        let result = QueryParam::<String, true>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_required_query_prarm_extract_with_value_panic() {
        let req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        let result = QueryParam::<String, true>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[test]
    fn test_query_param_metadata() {
        let metadata = QueryParam::<String, false>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_query_prarm_extract() {
        let mut req = Request::new();
        let _ = QueryParam::<String, false>::extract(&mut req).await;
    }

    #[tokio::test]
    async fn test_query_prarm_extract_with_value() {
        let req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        req.queries_mut()
            .insert("param".to_string(), "param".to_string());
        let result = QueryParam::<String, false>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_query_prarm_extract_with_value_panic() {
        let req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        let result = QueryParam::<String, false>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[test]
    fn test_query_param_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        QueryParam::<String, false>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "parameters": [
                    {
                        "name": "arg",
                        "in": "query",
                        "description": "Get parameter `arg` from request url query.",
                        "required": false,
                        "schema": {
                            "type": "string"
                        }
                    }
                ],
                "responses": {}
            })
        )
    }
}
