//! Extract supported types.

use serde::Deserialize;

/// Metadata types.
pub mod metadata;
pub use metadata::Metadata;

/// If a type implements this trait, it will give a metadata, this will help request to extracts data to this type.
pub trait Extractible<'de>: Deserialize<'de> {
    /// Metadata for Extractible type.
    fn metadata() -> &'de Metadata;
}
