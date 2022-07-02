use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::Deserialize;

use crate::http::ParseError;
use crate::Request;

pub mod metadata;
pub use metadata::Metadata;

pub trait Extractible<'de>: Deserialize<'de> {
    fn metadata() -> &'de Metadata;
}
