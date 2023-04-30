use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::ParseError;
use salvo_core::{async_trait, Request};
use serde::{Deserialize, Deserializer};

use crate::endpoint::EndpointArgRegister;
use crate::{ToRequestBody, ToSchema, Components, Content, Operation, Ref, RefOr, RequestBody};

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

impl<'de, T> ToRequestBody for JsonBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn to_request_body() -> RequestBody {
        let refor = if let Some(symbol) = <T as ToSchema>::to_schema().0 {
            RefOr::Ref(Ref::new(format!("#/components/schemas/{symbol}")))
        } else {
            T::to_schema().1
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
impl<'de, T> EndpointArgRegister for JsonBody<T>
where
    T: Deserialize<'de> + ToSchema,
{
    fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
        let request_body = Self::to_request_body();
        if let (Some(symbol), schema) = <T as ToSchema>::to_schema() {
            components.schemas.insert(symbol, schema);
        }
        operation.request_body = Some(request_body);
    }
}
