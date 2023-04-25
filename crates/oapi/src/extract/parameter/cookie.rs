use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use salvo_core::serde::from_str_val;
use salvo_core::{async_trait, Request};
use serde::Deserialize;
use serde::Deserializer;

use crate::endpoint::EndpointModifier;
use crate::{AsParameter, Components, Operation, Parameter, ParameterIn};

/// Represents the parameters passed by Cookie.
pub struct CookieParam<T> {
    name: String,
    value: T,
}
impl<T> CookieParam<T> {
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

impl<T> Deref for CookieParam<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for CookieParam<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> AsParameter for CookieParam<T> {
    fn parameter() -> Parameter {
        panic!("cookie parameter must have a argument");
    }
    fn parameter_with_arg(arg: &str) -> Parameter {
        Parameter::new(arg)
            .parameter_in(ParameterIn::Cookie)
            .description(format!("Get parameter `{arg}` from request cookie"))
    }
}

impl<'de, T> Deserialize<'de> for CookieParam<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(|value| CookieParam {
            name: "unknown".into(),
            value,
        })
    }
}

impl<T> fmt::Debug for CookieParam<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieParam")
            .field("name", &self.name)
            .field("value", &self.value)
            .finish()
    }
}

#[async_trait]
impl<'de, T> Extractible<'de> for CookieParam<T>
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
        Ok(Self {
            name: arg.to_string(),
            value,
        })
    }
}

#[async_trait]
impl<T> EndpointModifier for CookieParam<T> {
    fn modify(_components: &mut Components, _operation: &mut Operation) {
        panic!("cookie parameter can not modiify operation without argument");
    }
    fn modify_with_arg(_components: &mut Components, operation: &mut Operation, arg: &str) {
        operation.parameters.insert(Self::parameter_with_arg(arg));
    }
}
