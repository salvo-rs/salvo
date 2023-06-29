use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use salvo_core::serde::from_str_val;
use salvo_core::{async_trait, Request};
use serde::Deserialize;
use serde::Deserializer;

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

#[async_trait]
impl<'de, T> Extractible<'de> for CookieParam<T, true>
where
    T: Deserialize<'de>,
{
    fn metadata() -> &'de Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(_req: &'de mut Request) -> Result<Self, ParseError> {
        unimplemented!("cookie parameter can not be extracted from request")
    }
    async fn extract_with_arg(req: &'de mut Request, arg: &str) -> Result<Self, ParseError> {
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

#[async_trait]
impl<'de, T> Extractible<'de> for CookieParam<T, false>
where
    T: Deserialize<'de>,
{
    fn metadata() -> &'de Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(_req: &'de mut Request) -> Result<Self, ParseError> {
        unimplemented!("cookie parameter can not be extracted from request")
    }
    async fn extract_with_arg(req: &'de mut Request, arg: &str) -> Result<Self, ParseError> {
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
