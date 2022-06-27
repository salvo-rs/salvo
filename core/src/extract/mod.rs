
use std::collections::HashMap;

use multimap::MultiMap;

use crate::http::ParseError;

pub trait Extractor {
    type Output,
    async fn extract(&self, req: &mut Request) -> Result<Self::Output, ExtractError>;
}

pub struct ExtractorImpl<T>;

impl<T> Extractor for ExtractorImpl<T> wehre T: Extractible {
    type Output = T;
    async fn extract(&self, req: &mut Request) -> Result<Self::Output, ParseError> {
        let mut all_data: HashMap<&str, FieldValue> = HashMap::new();
        let metadata = ;
        if any_use_from {
            self.form_data().await?;
        }
        for field in &meta.fields {
           
        }
        from_request(req, Output::metadata()).map_err(ParseError::Deserialize)
    }
}

pub trait Extractible {
    fn metadata() -> &'static Metadata;
}


pub struct FieldValue<'a> {
    pub value: &'a str,
    pub format: Option<'a str>,
}
