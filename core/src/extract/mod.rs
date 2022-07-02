use serde::Deserialize;

pub mod metadata;
pub use metadata::Metadata;

pub trait Extractible<'de>: Deserialize<'de> {
    fn metadata() -> &'de Metadata;
}
