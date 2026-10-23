//! Form parse module.
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::engine::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::{Bytes, BytesMut};
use futures_util::stream::{Stream, TryStreamExt};
use mime::Mime;
use multimap::MultiMap;
use multra::{Field, Multipart};
use rand::TryRng;
use rand::rngs::SysRng;
use tempfile::Builder;

use crate::http::ParseError;
use crate::http::header::{CONTENT_TYPE, HeaderMap};
use crate::{Depot, FlowCtrl, Handler, Request, Response, async_trait};

/// Limits on the number of multipart parts and files parsed by [`Request::form_data`].
///
/// Defaults to 128 parts in total, including unnamed parts, and 32 file parts.
/// A zero limit rejects every part of that kind. These counts supplement the
/// request's byte limit; URL-encoded forms are governed by the byte limit only.
///
/// This type also implements [`Handler`]. Install it before the first form read
/// to customize a route, or use [`Request::set_form_limits`] inside a handler.
/// Already parsed, cached forms are not checked again. Direct calls to
/// [`FilePart::create`] are outside these request-level limits.
///
/// ```
/// use salvo_core::Router;
/// use salvo_core::http::form::FormLimits;
///
/// let uploads = Router::with_path("uploads").hoop(FormLimits::new().max_parts(256).max_files(64));
/// # let _ = uploads;
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct FormLimits {
    /// Maximum total number of multipart parts, including file and unnamed parts.
    pub max_parts: usize,
    /// Maximum number of multipart parts with a filename, including empty files.
    pub max_files: usize,
}

impl Default for FormLimits {
    fn default() -> Self {
        Self::new()
    }
}

impl FormLimits {
    /// Creates the default limits: 128 parts and 32 files.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_parts: 128,
            max_files: 32,
        }
    }

    /// Sets the maximum total part count. Use `usize::MAX` to remove this bound.
    #[must_use]
    pub const fn max_parts(mut self, max_parts: usize) -> Self {
        self.max_parts = max_parts;
        self
    }

    /// Sets the maximum file count. Use `usize::MAX` to remove this bound.
    #[must_use]
    pub const fn max_files(mut self, max_files: usize) -> Self {
        self.max_files = max_files;
        self
    }
}

#[async_trait]
impl Handler for FormLimits {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        _res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        req.set_form_limits(*self);
    }
}

/// The extracted text fields and uploaded files from a `multipart/form-data` request.
#[derive(Debug)]
#[non_exhaustive]
pub struct FormData {
    /// Name-value pairs for plain text fields. Technically, these are form data parts with no
    /// filename specified in the part's `Content-Disposition`.
    pub fields: MultiMap<String, String>,
    /// Name-value pairs for temporary files. Technically, these are form data parts with a
    /// filename specified in the part's `Content-Disposition`.
    pub files: MultiMap<String, FilePart>,
}

impl FormData {
    /// Creates a new `FormData`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            fields: MultiMap::new(),
            files: MultiMap::new(),
        }
    }

    /// Parse MIME `multipart/*` information from a stream as a `FormData`.
    pub(crate) async fn read<S, O, E>(
        headers: &HeaderMap,
        body: S,
        limits: FormLimits,
    ) -> Result<Self, ParseError>
    where
        S: Stream<Item = Result<O, E>> + Send,
        O: Into<Bytes> + 'static,
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Self::read_with_directory(headers, body, limits, temporary_directory).await
    }

    async fn read_with_directory<S, O, E>(
        headers: &HeaderMap,
        body: S,
        limits: FormLimits,
        create_directory: impl Fn() -> std::io::Result<tempfile::TempDir> + Clone + Send + 'static,
    ) -> Result<Self, ParseError>
    where
        S: Stream<Item = Result<O, E>> + Send,
        O: Into<Bytes> + 'static,
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let content_type: Option<Mime> = headers
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok());
        match content_type {
            Some(content_type) if content_type.subtype() == mime::WWW_FORM_URLENCODED => {
                futures_util::pin_mut!(body);
                let mut data = BytesMut::new();
                while let Some(chunk) = body.try_next().await.map_err(|e| {
                    let err = e.into();
                    if err.is::<http_body_util::LengthLimitError>() {
                        ParseError::PayloadTooLarge
                    } else {
                        ParseError::other(err)
                    }
                })? {
                    data.extend_from_slice(&chunk.into());
                }
                let mut form_data = Self::new();
                form_data.fields = form_urlencoded::parse(&data).into_owned().collect();
                Ok(form_data)
            }
            Some(content_type) if content_type.type_() == mime::MULTIPART => {
                let mut form_data = Self::new();
                let Some(boundary) = headers
                    .get(CONTENT_TYPE)
                    .and_then(|ct| ct.to_str().ok())
                    .and_then(|ct| multra::parse_boundary(ct).ok())
                else {
                    return Err(ParseError::InvalidContentType);
                };
                let mut multipart = Multipart::new(body, boundary);
                let mut parts = 0;
                let mut files = 0;
                while let Some(mut field) = multipart.next_field().await? {
                    if parts >= limits.max_parts {
                        return Err(ParseError::PayloadTooLarge);
                    }
                    parts += 1;
                    if field.file_name().is_some() {
                        if files >= limits.max_files {
                            return Err(ParseError::PayloadTooLarge);
                        }
                        files += 1;
                    }
                    if let Some(name) = field.name().map(|s| s.to_owned()) {
                        if field.file_name().is_some() {
                            form_data.files.insert(
                                name,
                                FilePart::create_with_directory(
                                    &mut field,
                                    create_directory.clone(),
                                )
                                .await?,
                            );
                        } else {
                            form_data.fields.insert(name, field.text().await?);
                        }
                    }
                }
                Ok(form_data)
            }
            _ => Err(ParseError::InvalidContentType),
        }
    }
}
impl Default for FormData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
/// A file that is to be inserted into a `multipart/*` or alternatively an uploaded file that
/// was received as part of `multipart/*` parsing.
#[derive(Clone, Debug)]
pub struct FilePart {
    name: Option<String>,
    /// The headers of the part
    headers: HeaderMap,
    /// A temporary file containing the file content
    path: PathBuf,
    /// Optionally, the size of the file.  This is filled when multiparts are parsed, but is
    /// not necessary when they are generated.
    size: u64,
    // Cleanup belongs to the upload, rather than any particular FilePart clone.
    storage: Arc<TemporaryUpload>,
}

#[derive(Debug)]
struct TemporaryUpload {
    path: PathBuf,
    temp_dir: PathBuf,
    delete_on_drop: AtomicBool,
}

// Field order closes the file before releasing the cleanup guard. Keeping both
// in the blocking task also covers cancellation while a disk operation runs.
struct UploadWriter {
    file: std::fs::File,
    storage: Arc<TemporaryUpload>,
}
impl FilePart {
    /// Get file name.
    #[inline]
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    /// Get file size.
    #[inline]
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }
    /// If you do not want the file on disk to be deleted when Self drops, call this
    /// function.  It will become your responsibility to clean up.
    ///
    /// This applies to all clones of this file part. Otherwise the file is removed
    /// after the last clone is dropped.
    #[inline]
    pub fn do_not_delete_on_drop(&mut self) {
        self.storage.delete_on_drop.store(false, Ordering::Relaxed);
    }

    /// Create a new temporary FilePart (when created this way, the file will be
    /// deleted once the FilePart object goes out of scope).
    ///
    /// # Security
    ///
    /// This streams the entire field to a temporary file with **no size limit
    /// of its own** — it writes until the field ends. When the body is read for
    /// the first time through
    /// [`Request::form_data`](crate::http::Request::form_data) it is bounded by
    /// the request's secure-max-size limit (configurable via
    /// [`Request::set_secure_max_size`](crate::http::Request::set_secure_max_size),
    /// the [`set_global_secure_max_size`](crate::http::request::set_global_secure_max_size)
    /// default, or the `SecureMaxSize` middleware).
    ///
    /// That guarantee only covers the **first** read of the body: if the body
    /// was already consumed and cached by an earlier `payload`/`payload_with_max_size`
    /// call (possibly under a larger limit or none), `form_data` reuses the
    /// cached payload without re-applying the limit. In that case, or when
    /// calling `create` directly on an otherwise unbounded `Field`, a client can
    /// fill the temporary directory's disk (denial of service), so make sure the
    /// request body is bounded before the first read.
    pub async fn create(field: &mut Field<'_>) -> Result<Self, ParseError> {
        Self::create_with_directory(field, temporary_directory).await
    }

    async fn create_with_directory(
        field: &mut Field<'_>,
        create_directory: impl FnOnce() -> std::io::Result<tempfile::TempDir> + Send + 'static,
    ) -> Result<Self, ParseError> {
        let name = field.file_name().map(|s| {
            // Sanitize filename by removing invalid characters
            s.chars()
                .filter(|c| {
                    !matches!(
                        c,
                        '/' | '\\' | '\0' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
                    )
                })
                .collect::<String>()
        });
        let extension = name
            .as_deref()
            .and_then(|name| Path::new(name).extension().and_then(OsStr::to_str))
            .unwrap_or("unknown")
            .to_owned();
        let mut writer = tokio::task::spawn_blocking(move || -> std::io::Result<UploadWriter> {
            let directory = create_directory()?;
            let path = directory
                .path()
                .join(format!("{}.{}", text_nonce(), extension));
            let storage = Arc::new(TemporaryUpload {
                path: path.clone(),
                temp_dir: directory.keep(),
                delete_on_drop: AtomicBool::new(true),
            });
            let file = std::fs::File::create(path)?;
            Ok(UploadWriter { file, storage })
        })
        .await
        .map_err(ParseError::other)??;
        let mut size = 0;
        while let Some(chunk) = field.chunk().await? {
            size += chunk.len() as u64;
            writer = tokio::task::spawn_blocking(move || -> std::io::Result<UploadWriter> {
                writer.file.write_all(&chunk)?;
                Ok(writer)
            })
            .await
            .map_err(ParseError::other)??;
        }
        let storage =
            tokio::task::spawn_blocking(move || -> std::io::Result<Arc<TemporaryUpload>> {
                writer.file.sync_all()?;
                let UploadWriter { file, storage } = writer;
                drop(file);
                Ok(storage)
            })
            .await
            .map_err(ParseError::other)??;
        Ok(Self {
            name,
            headers: field.headers().to_owned(),
            path: storage.path.clone(),
            size,
            storage,
        })
    }
}

fn temporary_directory() -> std::io::Result<tempfile::TempDir> {
    Builder::new().prefix("salvo_http_multipart").tempdir()
}

fn cleanup_temporary_upload(path: &Path, temp_dir: &Path) {
    // Log warnings if cleanup fails to help identify potential disk space issues.
    if let Err(e) = std::fs::remove_file(path) {
        // Only log if the file still exists (ENOENT is expected if already cleaned up).
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "failed to remove temporary upload file"
            );
        }
    }
    if let Err(e) = std::fs::remove_dir(temp_dir) {
        // Only log if directory still exists and is not empty.
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::debug!(
                error = %e,
                path = %temp_dir.display(),
                "failed to remove temporary upload directory"
            );
        }
    }
}

impl Drop for TemporaryUpload {
    fn drop(&mut self) {
        if self.delete_on_drop.load(Ordering::Relaxed) {
            let temp_dir = self.temp_dir.clone();
            let path = self.path.clone();
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::task::spawn_blocking(move || cleanup_temporary_upload(&path, &temp_dir));
            } else {
                cleanup_temporary_upload(&path, &temp_dir);
            }
        }
    }
}

// Port from https://github.com/mikedilger/textnonce/blob/master/src/lib.rs
fn text_nonce() -> String {
    const BYTE_LEN: usize = 24;
    let mut raw = [0u8; BYTE_LEN];

    // First 12 bytes are derived from a monotonically advancing time source so
    // that nonce values issued within the same process tend to differ even on
    // RNG failure; the trailing 12 bytes are pure CSPRNG output. If the RNG
    // fails (extremely rare), we fall back to a wider time window so we still
    // emit a valid nonce instead of panicking.
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        raw[..4].copy_from_slice(&now.subsec_nanos().to_le_bytes());
        raw[4..12].copy_from_slice(&now.as_secs().to_le_bytes());
    }
    if SysRng.try_fill_bytes(&mut raw[12..]).is_err() {
        let micros = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or_default();
        raw[12..20].copy_from_slice(&micros.to_le_bytes());
        raw[20..].copy_from_slice(&micros.rotate_left(17).to_le_bytes()[..4]);
    }

    URL_SAFE_NO_PAD.encode(raw)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn multipart_default_counts_reject_many_small_parts() {
        use crate::test::TestClient;

        for (parts, filename) in [(129, ""), (33, "; filename=\"x\"")] {
            let part =
                format!("--b\r\nContent-Disposition: form-data; name=\"f\"{filename}\r\n\r\n\r\n");
            let body = format!("{}--b--\r\n", part.repeat(parts));
            assert!(body.len() < 64 * 1024);
            let mut req = TestClient::post("http://localhost/")
                .add_header("content-type", "multipart/form-data; boundary=b", true)
                .body(body)
                .build();
            assert!(
                matches!(req.form_data().await, Err(ParseError::PayloadTooLarge)),
                "must reject {parts} multipart parts before the byte limit is reached"
            );
        }
    }

    fn multipart_request(parts: usize, files: usize) -> Request {
        let mut body = String::new();
        for index in 0..parts {
            let filename = if index < files {
                "; filename=\"x\""
            } else {
                ""
            };
            body.push_str(&format!(
                "--b\r\nContent-Disposition: form-data; name=\"f\"{filename}\r\n\r\n\r\n"
            ));
        }
        body.push_str("--b--\r\n");
        crate::test::TestClient::post("http://localhost/")
            .add_header("content-type", "multipart/form-data; boundary=b", true)
            .body(body)
            .build()
    }

    #[tokio::test]
    async fn multipart_count_boundaries_and_custom_limits() {
        for (parts, files, limits) in [
            (128, 0, FormLimits::default()),
            (32, 32, FormLimits::default()),
            (3, 2, FormLimits::new().max_parts(3).max_files(2)),
            (129, 0, FormLimits::new().max_parts(129)),
            (33, 33, FormLimits::new().max_files(33)),
            (1, 0, FormLimits::new().max_files(0)),
        ] {
            let mut req = multipart_request(parts, files);
            req.set_form_limits(limits);
            assert_eq!(req.form_limits(), limits);
            let form = req.form_data().await.unwrap();
            let count = form.files.get_vec("f").map_or(0, Vec::len)
                + form.fields.get_vec("f").map_or(0, Vec::len);
            assert_eq!(count, parts);
        }
        for (parts, files, limits) in [
            (4, 0, FormLimits::new().max_parts(3)),
            (3, 3, FormLimits::new().max_files(2)),
            (1, 0, FormLimits::new().max_parts(0)),
            (1, 1, FormLimits::new().max_files(0)),
        ] {
            let mut req = multipart_request(parts, files);
            req.set_form_limits(limits);
            assert!(matches!(
                req.form_data().await,
                Err(ParseError::PayloadTooLarge)
            ));
        }
    }

    #[tokio::test]
    async fn multipart_count_limits_cover_cached_payload_and_unnamed_parts() {
        let mut req = multipart_request(3, 0);
        req.payload().await.unwrap();
        req.set_form_limits(FormLimits::new().max_parts(2));
        assert!(matches!(
            req.form_data().await,
            Err(ParseError::PayloadTooLarge)
        ));

        let mut req = crate::test::TestClient::post("http://localhost/")
            .add_header("content-type", "multipart/form-data; boundary=b", true)
            .body("--b\r\nContent-Disposition: form-data\r\n\r\na\r\n--b\r\nContent-Disposition: form-data\r\n\r\nb\r\n--b--\r\n")
            .build();
        req.set_form_limits(FormLimits::new().max_parts(1));
        assert!(matches!(
            req.form_data().await,
            Err(ParseError::PayloadTooLarge)
        ));

        let mut req = crate::test::TestClient::post("http://localhost/")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .body("a=1&b=2")
            .build();
        req.set_form_limits(FormLimits::new().max_parts(0).max_files(0));
        assert_eq!(req.form_data().await.unwrap().fields.len(), 2);
    }

    #[tokio::test]
    async fn multipart_count_middleware_returns_413() {
        #[crate::handler]
        async fn parse(req: &mut Request) -> Result<&'static str, ParseError> {
            req.form_data().await?;
            Ok("ok")
        }
        let router = crate::Router::new()
            .hoop(FormLimits::new().max_parts(1).max_files(0))
            .post(parse);
        let service = crate::Service::new(router);
        for (parts, files, status) in [
            (1, 0, crate::http::StatusCode::OK),
            (2, 0, crate::http::StatusCode::PAYLOAD_TOO_LARGE),
            (1, 1, crate::http::StatusCode::PAYLOAD_TOO_LARGE),
        ] {
            let response = service.handle(multipart_request(parts, files)).await;
            assert_eq!(response.status_code, Some(status));
        }
    }

    #[tokio::test]
    async fn multipart_count_overflow_rejects_before_creation_and_cleans_previous_files() {
        use std::sync::atomic::AtomicUsize;

        use http_body_util::BodyExt;

        for limits in [
            FormLimits::new().max_parts(1),
            FormLimits::new().max_files(1),
        ] {
            let root = Builder::new().tempdir().unwrap();
            let root_path = root.path().to_owned();
            let created = Arc::new(AtomicUsize::new(0));
            let count = created.clone();
            let mut req = multipart_request(2, 2);
            let headers = req.headers().clone();
            let result = FormData::read_with_directory(
                &headers,
                req.take_body().into_data_stream(),
                limits,
                move || {
                    count.fetch_add(1, Ordering::Relaxed);
                    Builder::new().tempdir_in(&root_path)
                },
            )
            .await;
            assert!(matches!(result, Err(ParseError::PayloadTooLarge)));
            assert_eq!(created.load(Ordering::Relaxed), 1);
            wait_until(|| std::fs::read_dir(root.path()).unwrap().next().is_none()).await;
        }
    }

    #[tokio::test]
    async fn cancelling_multipart_parse_cleans_accumulated_files() {
        use std::sync::atomic::AtomicUsize;

        let root = Builder::new().tempdir().unwrap();
        let root_path = root.path().to_owned();
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        sender.send(Ok::<_, std::io::Error>(Bytes::from_static(
            b"--b\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x\"\r\n\r\ndata\r\n--b\r\nContent-Disposition: form-data; name=\"pending\"; filename=\"y\"\r\n\r\nwaiting",
        ))).await.unwrap();
        let second_file_started = Arc::new(tokio::sync::Notify::new());
        let notify = second_file_started.clone();
        let created = Arc::new(AtomicUsize::new(0));
        let count = created.clone();
        let first_directory = Arc::new(std::sync::Mutex::new(None));
        let first = first_directory.clone();
        let stream = futures_util::stream::unfold(receiver, |mut receiver| async {
            receiver.recv().await.map(|frame| (frame, receiver))
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            "multipart/form-data; boundary=b".parse().unwrap(),
        );
        let mut parse = Box::pin(FormData::read_with_directory(
            &headers,
            stream,
            FormLimits::default(),
            move || {
                let directory = Builder::new().tempdir_in(&root_path)?;
                match count.fetch_add(1, Ordering::Relaxed) {
                    0 => *first.lock().unwrap() = Some(directory.path().to_owned()),
                    // FormData creates files sequentially, so the first FilePart
                    // is complete and stored before it starts creating the second.
                    // Stream polls cannot establish this: multra may read ahead.
                    1 => notify.notify_one(),
                    _ => panic!("unexpected third file"),
                }
                Ok(directory)
            },
        ));
        tokio::select! {
            result = &mut parse => panic!("multipart parse finished before stream ended: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(5), second_file_started.notified()) => {
                result.expect("parser did not start the second file");
            }
        }
        assert_eq!(created.load(Ordering::Relaxed), 2);
        let directory = first_directory.lock().unwrap().clone().unwrap();
        let files = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        assert_eq!(std::fs::read(&files[0]).unwrap(), b"data");
        drop(parse);
        drop(sender);
        wait_until(|| std::fs::read_dir(root.path()).unwrap().next().is_none()).await;
    }

    fn file_part_in(root: &Path) -> FilePart {
        let temp_dir = Builder::new().tempdir_in(root).unwrap().keep();
        let path = temp_dir.join("upload.tmp");
        std::fs::write(&path, b"data").unwrap();
        FilePart {
            name: None,
            headers: HeaderMap::new(),
            path: path.clone(),
            size: 4,
            storage: Arc::new(TemporaryUpload {
                path,
                temp_dir,
                delete_on_drop: AtomicBool::new(true),
            }),
        }
    }

    #[tokio::test]
    async fn multipart_limits_map_to_413_at_every_read_stage() {
        use http_body_util::StreamBody;

        use crate::http::body::{Frame, ReqBody};
        use crate::test::TestClient;
        use crate::{Depot, Response, Writer};

        for file in [false, true] {
            let filename = if file {
                "; filename=\"upload.txt\""
            } else {
                ""
            };
            let first =
                format!("--b\r\nContent-Disposition: form-data; name=\"data\"{filename}\r\n\r\n");
            let second = "x".repeat(100);
            let third =
                "\r\n--b\r\nContent-Disposition: form-data; name=\"next\"\r\n\r\nnext\r\n--b--\r\n";
            // Fail in the first header, field contents, and next field header.
            for limit in [
                first.len() - 1,
                first.len() + 10,
                first.len() + second.len() + 10,
            ] {
                let frames = [first.clone(), second.clone(), third.to_owned()]
                    .into_iter()
                    .map(|chunk| Ok::<_, crate::BoxedError>(Frame::data(Bytes::from(chunk))));
                let body = ReqBody::Boxed {
                    inner: Box::pin(StreamBody::new(futures_util::stream::iter(frames))),
                    fuse_config: None,
                };
                let mut req = TestClient::post("http://localhost/")
                    .add_header("content-type", "multipart/form-data; boundary=b", true)
                    .body(body)
                    .build();
                req.set_secure_max_size(limit);
                let error = req.form_data().await.unwrap_err();
                assert!(
                    matches!(error, ParseError::PayloadTooLarge),
                    "file={file}, limit={limit}: {error:?}"
                );
                let mut response = Response::default();
                error
                    .write(&mut req, &mut Depot::new(), &mut response)
                    .await;
                assert_eq!(
                    response.status_code,
                    Some(crate::http::StatusCode::PAYLOAD_TOO_LARGE)
                );
            }
        }
    }

    #[tokio::test]
    async fn wrapped_multipart_limit_cleans_up_partial_file() {
        use http_body_util::{BodyExt, Full, Limited};
        let limit_error = Limited::new(Full::new(Bytes::from_static(b"large")), 1)
            .collect()
            .await
            .unwrap_err();
        let root = Builder::new().tempdir().unwrap();
        let root_path = root.path().to_owned();
        let stream = futures_util::stream::iter([
            Ok(Bytes::from_static(b"--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.txt\"\r\n\r\npartial")),
            Err(std::io::Error::other(limit_error)),
        ]);
        let mut multipart = Multipart::new(stream, "b");
        let mut field = multipart.next_field().await.unwrap().unwrap();
        let error = FilePart::create_with_directory(&mut field, move || {
            Builder::new().tempdir_in(root_path)
        })
        .await
        .unwrap_err();
        assert!(matches!(error, ParseError::PayloadTooLarge));
        wait_until(|| std::fs::read_dir(root.path()).unwrap().next().is_none()).await;
        assert!(matches!(
            ParseError::from(multra::Error::IncompleteStream),
            ParseError::Multer(_)
        ));
    }

    #[test]
    fn file_part_drop_removes_temp_files_without_tokio_runtime() {
        let root = Builder::new().tempdir().unwrap();
        let file_part = file_part_in(root.path());
        let path = file_part.path().clone();
        let temp_dir = path.parent().unwrap().to_owned();
        drop(file_part);

        assert!(!path.exists());
        assert!(!temp_dir.exists());
    }

    #[test]
    fn file_part_clones_keep_upload_until_last_owner_drops() {
        let root = Builder::new().tempdir().unwrap();
        let file_part = file_part_in(root.path());
        let path = file_part.path().clone();
        let retained = file_part.clone();
        drop(file_part.clone());
        assert_eq!(std::fs::read(&path).unwrap(), b"data");
        drop(file_part);
        assert_eq!(std::fs::read(retained.path()).unwrap(), b"data");
        drop(retained);
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }

    #[test]
    fn retaining_file_part_disables_cleanup_for_all_clones() {
        let root = Builder::new().tempdir().unwrap();
        let file_part = file_part_in(root.path());
        let path = file_part.path().clone();
        let mut retained = file_part.clone();
        retained.do_not_delete_on_drop();
        drop(retained);
        drop(file_part);
        assert_eq!(std::fs::read(path).unwrap(), b"data");
    }

    async fn wait_until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("temporary upload did not reach expected state");
    }

    fn contains_written_file(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().any(|directory| {
            std::fs::read_dir(directory.unwrap().path())
                .unwrap()
                .any(|file| {
                    // Query the open file rather than the directory entry: Windows can
                    // cache the entry's length until the writing handle closes.
                    std::fs::File::open(file.unwrap().path())
                        .unwrap()
                        .metadata()
                        .unwrap()
                        .len()
                        > 0
                })
        })
    }

    #[tokio::test]
    async fn failed_or_cancelled_multipart_write_removes_partial_upload() {
        for cancel in [false, true] {
            let root = Builder::new().tempdir().unwrap();
            let root_path = root.path().to_owned();
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            sender
                .send(Ok::<_, std::io::Error>(Bytes::from_static(
                    b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n\r\npartial upload",
                )))
                .await
                .unwrap();
            let stream = futures_util::stream::unfold(receiver, |mut receiver| async {
                receiver.recv().await.map(|frame| (frame, receiver))
            });
            let mut multipart = Multipart::new(stream, "boundary");
            let mut field = multipart.next_field().await.unwrap().unwrap();
            let mut create = Box::pin(FilePart::create_with_directory(&mut field, move || {
                Builder::new().tempdir_in(root_path)
            }));
            tokio::select! {
                result = &mut create => panic!("upload finished before source ended: {result:?}"),
                _ = wait_until(|| contains_written_file(root.path())) => {}
            }
            if cancel {
                drop(create);
            } else {
                sender
                    .send(Err(std::io::Error::other("client disconnected")))
                    .await
                    .unwrap();
                assert!(create.await.is_err());
            }
            wait_until(|| std::fs::read_dir(root.path()).unwrap().next().is_none()).await;
        }
    }

    #[tokio::test]
    async fn cancelled_blocking_operation_closes_file_before_cleanup() {
        let root = Builder::new().tempdir().unwrap();
        let file_part = file_part_in(root.path());
        let path = file_part.path().clone();
        let mut options = std::fs::OpenOptions::new();
        options.write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Windows must not delete the upload until this handle closes.
            options.share_mode(0);
        }
        let writer = UploadWriter {
            file: options.open(&path).unwrap(),
            storage: file_part.storage.clone(),
        };
        drop(file_part);
        let (started, start) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let operation = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            released.recv().unwrap();
            writer
        });
        start.await.unwrap();
        drop(operation);
        assert!(
            path.exists(),
            "the blocking operation still owns the upload"
        );
        release.send(()).unwrap();
        wait_until(|| std::fs::read_dir(root.path()).unwrap().next().is_none()).await;
    }
}
