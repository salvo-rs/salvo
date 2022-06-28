
use std::collections::HashMap;
use std::marker::PhantomData;

use multimap::MultiMap;
use async_trait::async_trait;

use crate::http::ParseError;
use crate::Request;

pub mod metadata;
pub use metadata::Metadata;

#[async_trait]
pub trait Extractor {
    type Output;
    async fn extract(&self, req: &mut Request) -> Result<Self::Output, ParseError>;
}

pub struct ExtractorImpl<T>(PhantomData<T>);

#[async_trait]
impl<T> Extractor for ExtractorImpl<T> where T: Extractible {
    type Output = T;
    async fn extract(&self, req: &mut Request) -> Result<Self::Output, ParseError> {
        crate::serde::from_request(req, Self::Output::metadata()).map_err(ParseError::Deserialize)
    }
}

pub trait Extractible {
    fn metadata() -> &'static Metadata;
}