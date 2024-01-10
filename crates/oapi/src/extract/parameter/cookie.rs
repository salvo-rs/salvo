use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::{ParseError, Request};
use salvo_core::serde::from_str_val;
use serde::{Deserialize, Deserializer};

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Operation, Parameter, ParameterIn, ToSchema};

/// Represents the parameters passed by Cookie.
pub struct CookieParam<T, const REQUIRED: bool>(Option<T>);
impl<T> CookieParam<T, true> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0.unwrap()
    }
}
impl<T> CookieParam<T, false> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> Option<T> {
        self.0
    }
}

impl<T> Deref for CookieParam<T, true> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}
impl<T> Deref for CookieParam<T, false> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for CookieParam<T, true> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}
impl<T> DerefMut for CookieParam<T, false> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de, T, const R: bool> Deserialize<'de> for CookieParam<T, R>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(|value| CookieParam(Some(value)))
    }
}

impl<T, const R: bool> fmt::Debug for CookieParam<T, R>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> fmt::Display for CookieParam<T, true>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.as_ref().unwrap().fmt(f)
    }
}

impl<'ex, T> Extractible<'ex> for CookieParam<T, true>
where
    T: Deserialize<'ex>,
{
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request) -> Result<Self, ParseError> {
        unimplemented!("cookie parameter can not be extracted from request");
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(req: &'ex mut Request, arg: &str) -> Result<Self, ParseError> {
        let value = req
            .cookies()
            .get(arg)
            .and_then(|v| from_str_val(v.value()).ok())
            .ok_or_else(|| {
                ParseError::other(format!("cookie parameter {} not found or convert to type failed", arg))
            })?;
        Ok(Self(value))
    }
}

impl<'ex, T> Extractible<'ex> for CookieParam<T, false>
where
    T: Deserialize<'ex>,
{
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request) -> Result<Self, ParseError> {
        unimplemented!("cookie parameter can not be extracted from request")
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(req: &'ex mut Request, arg: &str) -> Result<Self, ParseError> {
        let value = req.cookies().get(arg).and_then(|v| from_str_val(v.value()).ok());
        Ok(Self(value))
    }
}

impl<T, const R: bool> EndpointArgRegister for CookieParam<T, R>
where
    T: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let parameter = Parameter::new(arg)
            .parameter_in(ParameterIn::Cookie)
            .description(format!("Get parameter `{arg}` from request cookie."))
            .schema(T::to_schema(components))
            .required(R);
        operation.parameters.insert(parameter);
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use http::header::HeaderValue;
    use salvo_core::test::TestClient;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_required_cookie_param_into_inner() {
        let param = CookieParam::<String, true>(Some("param".to_string()));
        assert_eq!("param".to_string(), param.into_inner());
    }

    #[test]
    fn test_required_cookie_param_deref() {
        let param = CookieParam::<String, true>(Some("param".to_string()));
        assert_eq!(&"param".to_string(), param.deref())
    }

    #[test]
    fn test_required_cookie_param_deref_mut() {
        let mut param = CookieParam::<String, true>(Some("param".to_string()));
        assert_eq!(&mut "param".to_string(), param.deref_mut())
    }

    #[test]
    fn test_cookie_param_into_inner() {
        let param = CookieParam::<String, false>(Some("param".to_string()));
        assert_eq!(Some("param".to_string()), param.into_inner());
    }

    #[test]
    fn test_cookie_param_deref() {
        let param = CookieParam::<String, false>(Some("param".to_string()));
        assert_eq!(&Some("param".to_string()), param.deref())
    }

    #[test]
    fn test_cookie_param_deref_mut() {
        let mut param = CookieParam::<String, false>(Some("param".to_string()));
        assert_eq!(&mut Some("param".to_string()), param.deref_mut())
    }

    #[test]
    fn test_cookie_param_deserialize() {
        let param = serde_json::from_str::<CookieParam<String, true>>(r#""param""#).unwrap();
        assert_eq!(param.0.unwrap(), "param");
    }

    #[test]
    fn test_cookie_param_debug() {
        let param = CookieParam::<String, true>(Some("param".to_string()));
        assert_eq!(format!("{:?}", param), r#"Some("param")"#);
    }

    #[test]
    fn test_cookie_param_display() {
        let param = CookieParam::<String, true>(Some("param".to_string()));
        assert_eq!(format!("{}", param), "param");
    }

    #[test]
    fn test_required_cookie_param_metadata() {
        let metadata = CookieParam::<String, true>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_required_cookie_prarm_extract() {
        let mut req = Request::new();
        let _ = CookieParam::<String, true>::extract(&mut req).await;
    }

    #[tokio::test]
    async fn test_required_cookie_prarm_extract_with_value() {
        let mut req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        req.headers_mut()
            .append("cookie", HeaderValue::from_static("param=param"));
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        let result = CookieParam::<String, true>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_required_cookie_prarm_extract_with_value_panic() {
        let req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        let result = CookieParam::<String, true>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[test]
    fn test_cookie_param_metadata() {
        let metadata = CookieParam::<String, false>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_cookie_prarm_extract() {
        let mut req = Request::new();
        let _ = CookieParam::<String, false>::extract(&mut req).await;
    }

    #[tokio::test]
    async fn test_cookie_prarm_extract_with_value() {
        let mut req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        req.headers_mut()
            .append("cookie", HeaderValue::from_static("param=param"));
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        let result = CookieParam::<String, false>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_cookie_prarm_extract_with_value_panic() {
        let req = TestClient::get("http://127.0.0.1:5801").build_hyper();
        let schema = req.uri().scheme().cloned().unwrap();
        let mut req = Request::from_hyper(req, schema);
        let result = CookieParam::<String, false>::extract_with_arg(&mut req, "param").await;
        assert_eq!(result.unwrap().0.unwrap(), "param");
    }

    #[test]
    fn test_cookie_param_register() {
        let mut components = Components::new();
        let mut operation = Operation::new();
        CookieParam::<String, false>::register(&mut components, &mut operation, "arg");

        assert_json_eq!(
            operation,
            json!({
                "parameters": [
                    {
                        "name": "arg",
                        "in": "cookie",
                        "description": "Get parameter `arg` from request cookie.",
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
