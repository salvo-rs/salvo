use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use salvo_core::{async_trait, Request};
use serde::Deserialize;
use serde::Deserializer;

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Operation, Parameter, ParameterIn, ToSchema};

/// Represents the parameters passed by header.
pub struct HeaderParam<T, const REQUIRED: bool>(Option<T>);
impl<T> HeaderParam<T, true> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0.unwrap()
    }
}
impl<T> HeaderParam<T, false> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> Option<T> {
        self.0
    }
}

impl<T> Deref for HeaderParam<T, true> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}
impl<T> Deref for HeaderParam<T, false> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for HeaderParam<T, true> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}
impl<T> DerefMut for HeaderParam<T, false> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de, T, const R: bool> Deserialize<'de> for HeaderParam<T, R>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(|value| HeaderParam(Some(value)))
    }
}
// impl<'de, T> Deserialize<'de> for HeaderParam<T, false>
// where
//     T: Deserialize<'de>,
// {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         T::deserialize(deserializer).map(|value| HeaderParam(Some(value)))
//     }
// }

impl<T, const R: bool> fmt::Debug for HeaderParam<T, R>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl<T> fmt::Display for HeaderParam<T, true>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.as_ref().unwrap().fmt(f)
    }
}

#[async_trait]
impl<'de, T> Extractible<'de> for HeaderParam<T, true>
where
    T: Deserialize<'de>,
{
    fn metadata() -> &'de Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(_req: &'de mut Request) -> Result<Self, ParseError> {
        unimplemented!("header parameter can not be extracted from request")
    }
    async fn extract_with_arg(req: &'de mut Request, arg: &str) -> Result<Self, ParseError> {
        let value = req.header(arg).ok_or_else(|| {
            ParseError::other(format!("header parameter {} not found or convert to type failed", arg))
        })?;
        Ok(Self(value))
    }
}
#[async_trait]
impl<'de, T> Extractible<'de> for HeaderParam<T, false>
where
    T: Deserialize<'de>,
{
    fn metadata() -> &'de Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(_req: &'de mut Request) -> Result<Self, ParseError> {
        unimplemented!("header parameter can not be extracted from request")
    }
    async fn extract_with_arg(req: &'de mut Request, arg: &str) -> Result<Self, ParseError> {
        Ok(Self(req.header(arg)))
    }
}

impl<T, const R: bool> EndpointArgRegister for HeaderParam<T, R>
where
    T: ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, arg: &str) {
        let parameter = Parameter::new(arg.trim_start_matches('_'))
            .parameter_in(ParameterIn::Header)
            .description(format!("Get parameter `{arg}` from request headers."))
            .schema(T::to_schema(components))
            .required(R);
        if arg.starts_with('_') {
            if !operation.parameters.contains(&parameter.name, parameter.parameter_in) {
                operation.parameters.insert(parameter);
            }
        } else {
            operation.parameters.insert(parameter);
        }
    }
}
