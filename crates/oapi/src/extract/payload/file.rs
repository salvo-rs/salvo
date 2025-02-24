use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::form::FilePart;
use salvo_core::http::header::CONTENT_TYPE;
use salvo_core::http::{HeaderMap, Mime, ParseError};
use salvo_core::{Request, async_trait};

use crate::endpoint::EndpointArgRegister;
use crate::{
    Array, BasicType, Components, Content, KnownFormat, Object, Operation, RequestBody, Schema,
    SchemaFormat,
};

/// Represents the upload file.
#[derive(Clone, Debug)]
pub struct FormFile {
    name: Option<String>,
    /// The headers of the part
    headers: HeaderMap,
    /// A temporary file containing the file content
    path: PathBuf,
    /// Optionally, the size of the file.  This is filled when multiparts are parsed, but is
    /// not necessary when they are generated.
    size: u64,
}
impl FormFile {
    /// Create a new `FormFile` from a `FilePart`.
    pub fn new(file_part: &FilePart) -> Self {
        Self {
            name: file_part.name().map(|s| s.to_owned()),
            headers: file_part.headers().clone(),
            path: file_part.path().to_owned(),
            size: file_part.size(),
        }
    }

    /// Get file name.
    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// Get file name mutable reference.
    #[inline]
    pub fn name_mut(&mut self) -> Option<&mut String> {
        self.name.as_mut()
    }
    /// Get headers.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    /// Get headers mutable reference.
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    /// Get content type.
    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        self.headers
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok())
    }
    /// Get file path.
    #[inline]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    /// Get file size.
    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl<'ex> Extractible<'ex> for FormFile {
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request) -> Result<Self, ParseError> {
        panic!("query parameter can not be extracted from request")
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(req: &'ex mut Request, arg: &str) -> Result<Self, ParseError> {
        req.file(arg)
            .await
            .map(FormFile::new)
            .ok_or_else(|| ParseError::other("file not found"))
    }
}

#[async_trait]
impl EndpointArgRegister for FormFile {
    fn register(_components: &mut Components, operation: &mut Operation, arg: &str) {
        let schema = Schema::from(
            Object::new().property(
                arg,
                Object::with_type(BasicType::String)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Binary)),
            ),
        );

        if let Some(request_body) = &mut operation.request_body {
            request_body
                .contents
                .insert("multipart/form-data".into(), Content::new(schema));
        } else {
            let request_body = RequestBody::new()
                .description("Upload a file.")
                .add_content("multipart/form-data", Content::new(schema));
            operation.request_body = Some(request_body);
        }
    }
}

/// Represents the upload files.
#[derive(Clone, Debug)]
pub struct FormFiles(pub Vec<FormFile>);
impl FormFiles {
    /// Create a new `FormFiles` from a `Vec<&FilePart>`.
    pub fn new(file_parts: Vec<&FilePart>) -> Self {
        Self(file_parts.into_iter().map(FormFile::new).collect())
    }

    /// Get inner files.
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
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    #[allow(refining_impl_trait)]
    async fn extract(_req: &'ex mut Request) -> Result<Self, ParseError> {
        panic!("query parameter can not be extracted from request")
    }
    #[allow(refining_impl_trait)]
    async fn extract_with_arg(req: &'ex mut Request, arg: &str) -> Result<Self, ParseError> {
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

#[async_trait]
impl EndpointArgRegister for FormFiles {
    fn register(_components: &mut Components, operation: &mut Operation, arg: &str) {
        let schema = Schema::from(
            Object::new().property(
                arg,
                Array::new().items(Schema::from(
                    Object::with_type(BasicType::String)
                        .format(SchemaFormat::KnownFormat(KnownFormat::Binary)),
                )),
            ),
        );
        if let Some(request_body) = &mut operation.request_body {
            request_body
                .contents
                .insert("multipart/form-data".into(), Content::new(schema));
        } else {
            let request_body = RequestBody::new()
                .description("Upload files.")
                .add_content("multipart/form-data", Content::new(schema));
            operation.request_body = Some(request_body);
        }
    }
}
