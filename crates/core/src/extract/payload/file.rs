use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

use crate::extract::{Extractible, Metadata};
use crate::http::form::FilePart;
use crate::http::{HeaderMap, Mime, ParseError};
use crate::{Depot, Request};

/// Represents the upload file.
///
/// Keeps its temporary upload alive until the last `FormFile` or `FilePart`
/// owner is dropped, including after the request has ended.
#[derive(Clone, Debug)]
pub struct FormFile {
    file_part: FilePart,
}
impl FormFile {
    /// Create a new `FormFile` from a `FilePart`.
    #[must_use]
    pub fn new(file_part: &FilePart) -> Self {
        Self {
            file_part: file_part.clone(),
        }
    }

    /// Get file name.
    #[inline]
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.file_part.name()
    }
    /// Get file name mutable reference.
    #[inline]
    pub fn name_mut(&mut self) -> Option<&mut String> {
        self.file_part.name_mut()
    }
    /// Get headers.
    #[inline]
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        self.file_part.headers()
    }
    /// Get headers mutable reference.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.file_part.headers_mut()
    }
    /// Get content type.
    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        self.file_part.content_type()
    }
    /// Get file path.
    #[inline]
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        self.file_part.path()
    }
    /// Get file size.
    #[inline]
    #[must_use]
    pub fn size(&self) -> u64 {
        self.file_part.size()
    }
}

impl<'ex> Extractible<'ex> for FormFile {
    fn metadata() -> &'static Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request, _depot: &'ex mut Depot) -> Result<Self, ParseError> {
        panic!("query parameter cannot be extracted from request")
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(
        req: &'ex mut Request,
        _depot: &'ex mut Depot,
        arg: &str,
    ) -> Result<Self, ParseError> {
        req.file(arg)
            .await
            .map(Self::new)
            .ok_or_else(|| ParseError::other("file not found"))
    }
}

/// Represents the upload files.
#[derive(Clone, Debug)]
pub struct FormFiles(pub Vec<FormFile>);
impl FormFiles {
    /// Create a new `FormFiles` from a `Vec<&FilePart>`.
    #[must_use]
    pub fn new(file_parts: Vec<&FilePart>) -> Self {
        Self(file_parts.into_iter().map(FormFile::new).collect())
    }

    /// Get inner files.
    #[must_use]
    pub fn into_inner(self) -> Vec<FormFile> {
        self.0
    }
}
impl Deref for FormFiles {
    type Target = Vec<FormFile>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FormFiles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'ex> Extractible<'ex> for FormFiles {
    fn metadata() -> &'static Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request, _depot: &'ex mut Depot) -> Result<Self, ParseError> {
        panic!("query parameter cannot be extracted from request")
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(
        req: &'ex mut Request,
        _depot: &'ex mut Depot,
        arg: &str,
    ) -> Result<Self, ParseError> {
        Ok(Self(
            req.files(arg)
                .await
                .ok_or_else(|| ParseError::other("file not found"))?
                .iter()
                .map(FormFile::new)
                .collect(),
        ))
    }
}
