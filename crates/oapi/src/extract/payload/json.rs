use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use salvo_core::{async_trait, Request};
use serde::{Deserialize, Deserializer};

use crate::endpoint::EndpointModifier;
use crate::{AsRequestBody, AsSchema, Components, Content, Operation, Ref, RefOr, RequestBody};

/// Represents the parameters passed by the URI path.
pub struct JsonBody<T>(pub T);

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

impl<'de, T> AsRequestBody for JsonBody<T>
where
    T: Deserialize<'de> + AsSchema,
{
    fn request_body() -> RequestBody {
        let refor = if let Some(symbol) = <T as AsSchema>::symbol() {
            RefOr::Ref(Ref::new(format!("#/components/schemas/{symbol}")))
        } else {
            T::schema()
        };
        RequestBody::new()
            .description("Extract json format data from request.")
            .add_content("application/json", Content::new(refor))
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

#[async_trait]
impl<'de, T> Extractible<'de> for JsonBody<T>
where
    T: Deserialize<'de> + Send,
{
    fn metadata() -> &'de Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(req: &'de mut Request) -> Result<Self, ParseError> {
        req.parse_json().await
    }
    async fn extract_with_arg(req: &'de mut Request, _arg: &str) -> Result<Self, ParseError> {
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

#[async_trait]
impl<'de, T> EndpointModifier for JsonBody<T>
where
    T: Deserialize<'de> + AsSchema,
{
    fn modify(components: &mut Components, operation: &mut Operation) {
        let request_body = Self::request_body();
        if let Some(symbol) = <T as AsSchema>::symbol() {
            components.schemas.insert(symbol.into(), <T as AsSchema>::schema());
        }
        operation.request_body = Some(request_body);
    }
}
