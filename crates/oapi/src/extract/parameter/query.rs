use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use salvo_core::{async_trait, Request};
use serde::Deserialize;
use serde::Deserializer;

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Operation, Parameter, ParameterIn, ToSchema};

/// Represents the parameters passed by the URI path.
pub struct QueryParam<T, const REQUIRED: bool>(Option<T>);
impl<T> QueryParam<T, true> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0.unwrap()
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
        self.0.as_ref().unwrap()
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
        self.0.as_mut().unwrap()
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

impl<T, const R: bool> fmt::Debug for QueryParam<T, R>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[async_trait]
impl<'de, T> Extractible<'de> for QueryParam<T, true>
where
    T: Deserialize<'de>,
{
    fn metadata() -> &'de Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(_req: &'de mut Request) -> Result<Self, ParseError> {
        panic!("query parameter can not be extracted from request")
    }
    async fn extract_with_arg(req: &'de mut Request, arg: &str) -> Result<Self, ParseError> {
        let value = req
            .query(arg)
            .ok_or_else(|| ParseError::other(format!("query parameter {} not found or convert to type failed", arg)))?;
        Ok(Self(value))
    }
}
#[async_trait]
impl<'de, T> Extractible<'de> for QueryParam<T, false>
where
    T: Deserialize<'de>,
{
    fn metadata() -> &'de Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(_req: &'de mut Request) -> Result<Self, ParseError> {
        panic!("query parameter can not be extracted from request")
    }
    async fn extract_with_arg(req: &'de mut Request, arg: &str) -> Result<Self, ParseError> {
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
