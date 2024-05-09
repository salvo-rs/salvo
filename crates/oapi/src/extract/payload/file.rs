use std::fmt::{self, Formatter};
use std::ops::{Deref, DerefMut};

use salvo_core::extract::{Extractible, Metadata};
use salvo_core::http::form::FilePart;
use salvo_core::{async_trait, Request, Writer};
use serde::{Deserialize, Deserializer};

use crate::endpoint::EndpointArgRegister;
use crate::{Components, Content, Operation, RequestBody, ToRequestBody, ToSchema};

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

impl ToRequestBody for FormFile {
    fn to_request_body(components: &mut Components) -> RequestBody {
        let schema =
            Schema::from(Object::with_type(SchemaType::String).format(SchemaFormat::KnownFormat(KnownFormat::Binary)));
        RequestBody::new()
            .description("Upload a file.")
            .add_content("multipart/form-data", Content::new(schema))
    }
}

impl Extractible for FormFile {
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(req: &'ex mut Request) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        panic!("query parameter can not be extracted from request")
    }
    async fn extract_with_arg(
        req: &'ex mut Request,
        arg: &str,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        req.file(arg).await.map(|file_part| FormFile::new(&file_part))
    }
}

#[async_trait]
impl EndpointArgRegister for FormFile {
    fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
        let request_body = Self::to_request_body(components);
        operation.request_body = Some(request_body);
    }
}



/// Represents the upload files.
#[derive(Clone, Debug)]
pub struct FormFiles(Vec<FormFile>);
impl FormFiles {
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

impl ToRequestBody for FormFile {
    fn to_request_body(components: &mut Components) -> RequestBody {
        let schema =
            Schema::from(Object::with_type(SchemaType::String).format(SchemaFormat::KnownFormat(KnownFormat::Binary)));
        RequestBody::new()
            .description("Upload a file.")
            .add_content("multipart/form-data", Content::new(schema))
    }
}

impl Extractible for FormFile {
    fn metadata() -> &'ex Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(req: &'ex mut Request) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        panic!("query parameter can not be extracted from request")
    }
    async fn extract_with_arg(
        req: &'ex mut Request,
        arg: &str,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        req.file(arg).await.map(|file_part| FormFile::new(&file_part))
    }
}

#[async_trait]
impl EndpointArgRegister for FormFile {
    fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
        let request_body = Self::to_request_body(components);
        operation.request_body = Some(request_body);
    }
}
