use std::collections::HashMap;
use std::marker::PhantomData;

use async_trait::async_trait;
use multimap::MultiMap;
use serde::Deserialize;

use crate::http::ParseError;
use crate::Request;

pub mod metadata;
pub use metadata::Metadata;

#[async_trait]
pub trait Extractor<'de> {
    type Output;
    async fn extract(&self, req: &'de mut Request) -> Result<Self::Output, ParseError>;
}

pub struct ExtractorImpl<T>(PhantomData<T>);

#[async_trait]
impl<'de, T> Extractor<'de> for ExtractorImpl<T>
where
    T: Extractible<'de> + Sync,
{
    type Output = T;
    async fn extract(&self, req: &'de mut Request) -> Result<Self::Output, ParseError> {
        crate::serde::from_request(req, Self::Output::metadata()).await
    }
}

pub trait Extractible<'de>: Deserialize<'de> {
    fn metadata() -> &'de Metadata;
}
