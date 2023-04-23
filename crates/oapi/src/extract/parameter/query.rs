use std::ops::{Deref, DerefMut};
use std::fmt::{self, Display, Formatter};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use salvo_core::{async_trait, Request};
use serde::Deserialize;
use serde::Deserializer;

use crate::endpoint::EndpointModifier;
use crate::{AsParameter, Components, Operation, Parameter, ParameterIn};

/// Represents the parameters passed by the URI path.
pub struct Query<T> {
    name: String,
    value: T,
}
impl<T> Query<T> {
    pub fn new(name: &str, value: T) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T> Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Query<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> AsParameter for Query<T> {
    fn parameter(arg: Option<&str>) -> Parameter {
        Parameter::new(arg.unwrap()).parameter_in(ParameterIn::Query)
    }
}

impl<'de, T> Deserialize<'de> for Query<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(|value| Query {
            name: "unknown".into(),
            value,
        })
    }
}

impl<T> fmt::Debug for Query<T> where T: fmt::Debug {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Query")
            .field("name", &self.name)
            .field("value", &self.value)
            .finish()
    }
}

#[async_trait]
impl<'de, T> Extractible<'de> for Query<T>
where
    T: Deserialize<'de>,
{
    fn metadata() -> &'de Metadata {
        panic!("metadata can not be extracted from `Query`")
    }
    async fn extract(req: &'de mut Request) -> Result<Self, ParseError> {
        panic!("query parameter can not be extracted from request")
    }
    async fn extract_with_arg(req: &'de mut Request, arg: &str) -> Result<Query<T>, ParseError> {
        let value = req.query(arg).ok_or_else(|| {
            ParseError::other(format!(
                "query parameter {} not found or convert to type failed",
                arg
            ))
        })?;
        Ok(Query {
            name: arg.to_string(),
            value,
        })
    }
}

#[async_trait]
impl<T> EndpointModifier for Query<T> {
    fn modify(components: &mut Components, operation: &mut Operation, arg: Option<&str>) {
        operation.parameters.append(Self::parameter(arg));
    }
}
