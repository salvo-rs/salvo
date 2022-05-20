
use std::fmt;
use std::io::{copy, prelude::*, Cursor, Error as IoError, Result as IoResult};

use mime::Mime;
use multipart::client as mc;

use crate::test::{Error, ErrorKind, Result};

/// A file to be uploaded as part of a multipart form.
#[derive(Debug, Clone)]
pub struct MultipartFile<'key, 'data> {
    name: &'key str,
    file: &'data [u8],
    filename: Option<&'key str>,
    mime: Option<Mime>,
}

impl<'key, 'data> MultipartFile<'key, 'data> {
    /// Constructs a new `MultipartFile` from the name and contents.
    pub fn new(name: &'key str, file: &'data [u8]) -> Self {
        Self {
            name,
            file,
            filename: None,
            mime: None,
        }
    }

    /// Sets the MIME type of the file.
    ///
    /// # Errors
    /// Returns an error if the MIME type is invalid.
    pub fn with_type(self, mime_type: impl AsRef<str>) -> Result<Self> {
        let mime_str = mime_type.as_ref();
        let mime: Mime = match mime_str.parse() {
            Ok(mime) => mime,
            Err(error) => return Err(Error(Box::new(ErrorKind::InvalidMimeType(error.to_string())))),
        };
        Ok(Self {
            mime: Some(mime),
            ..self
        })
    }

    /// Sets the filename of the file.
    pub fn with_filename(self, filename: &'key str) -> Self {
        Self {
            filename: Some(filename),
            ..self
        }
    }
}

/// A builder for creating a `Multipart` body.
#[derive(Debug, Clone, Default)]
pub struct MultipartBuilder<'key, 'data> {
    text: Vec<(&'key str, &'data str)>,
    files: Vec<MultipartFile<'key, 'data>>,
}

impl<'key, 'data> MultipartBuilder<'key, 'data> {
    /// Creates a new `MultipartBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a text field to the form.
    pub fn with_text(mut self, name: &'key str, text: &'data str) -> Self {
        self.text.push((name, text));
        self
    }

    /// Adds a `MultipartFile` to the form.
    pub fn with_file(mut self, file: MultipartFile<'key, 'data>) -> Self {
        self.files.push(file);
        self
    }

    /// Creates a `Multipart` to be used as a body.
    pub fn build(self) -> Result<Multipart<'data>> {
        let mut mc = mc::lazy::Multipart::new();
        for (k, v) in self.text {
            mc.add_text(k, v);
        }
        for file in self.files {
            mc.add_stream(file.name, Cursor::new(file.file), file.filename, file.mime);
        }
        let prepared = mc.prepare().map_err::<IoError, _>(Into::into)?;
        Ok(Multipart { data: prepared })
    }
}

/// A multipart form created using `MultipartBuilder`.
pub struct Multipart<'data> {
    data: mc::lazy::PreparedFields<'data>,
}

impl Body for Multipart<'_> {
    fn kind(&mut self) -> IoResult<BodyKind> {
        Ok(BodyKind::Chunked)
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        copy(&mut self.data, &mut writer)?;
        Ok(())
    }

    fn content_type(&mut self) -> IoResult<Option<String>> {
        Ok(Some(format!("multipart/form-data; boundary={}", self.data.boundary())))
    }
}

impl fmt::Debug for Multipart<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Multipart").finish()
    }
}
