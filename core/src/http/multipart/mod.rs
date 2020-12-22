use futures::{Stream, TryStream};
use http::header::HeaderMap;
use mime::Mime;
use std::pin::Pin;
use std::task::{Context, Poll};

use self::boundary::BoundaryFinder;
use self::field::ReadHeaders;
use crate::http::errors::ReadError;
use crate::http::BodyChunk;

#[cfg(test)]
#[macro_use]
pub mod test_util;

mod helpers;
pub use self::field::{Field, FieldData, FieldHeaders, NextField, ReadToString};

macro_rules! try_opt (
    ($expr:expr) => (
        match $expr {
            Some(val) => val,
            None => return None,
        }
    )
);

macro_rules! ret_err (
    ($($args:tt)+) => (
        return fmt_err!($($args)+).into();
    )
);

// macro_rules! ret_ok(
//     ($expr:expr) => (return Ok($expr).into());
// );

macro_rules! fmt_err (
    ($string:expr) => (
        Err($crate::http::errors::ReadError::Parsing($string.into()))
    );
    ($string:expr, $($args:tt)*) => (
        Err($crate::http::errors::ReadError::Parsing(format!($string, $($args)*).into()))
    );
);

mod boundary;
mod field;

/// The server-side implementation of `multipart/form-data` requests.
///
/// After constructing with either the [`::with_body()`](#method.with_body) or
/// [`::try_from_request()`](#method.try_from_request), two different workflows for processing the
/// request are provided, assuming any `Poll::Pending` and `Ready(Err(_))`/`Ready(Some(Err(_)))`
/// results are handled in the typical fashion:
///
/// ### High-Level Flow
///
/// 1. Await the next field with [`.next_field()`](#method.next_field).
/// 2. Read the field data via the `Stream` impl on `Field::data`.
/// 3. Repeat until `.next_field()` returns `None`.
///
/// ### Low-Level Flow
///
/// 1. Poll for the first field boundary with [`.poll_has_next_field()`](#method.poll_has_next_field);
/// if it returns `true` proceed to the next step, if `false` the request is at an end.
///
/// 2. Poll for the field's headers containing its name, content-type and other info with
/// [`.poll_field_headers()`](#method.poll_field_headers).
///
/// 3. Poll for the field's data chunks with [`.poll_field_chunk()](#method.poll_field_chunk)
/// until `None` is returned, then loop back to step 2.
///
/// Any data before the first boundary and past the end of the terminating boundary is ignored
/// as it is out-of-spec and should not be expected to be left in the underlying stream intact.
/// Please open an issue if you have a legitimate use-case for extraneous data in a multipart request.
pub struct Multipart<S: TryStream>
where
    S::Error: Into<ReadError>,
{
    inner: PushChunk<BoundaryFinder<S>, S::Ok>,
    read_hdr: ReadHeaders,
}

// Q: why can't we just wrap up these bounds into a trait?
// A: https://github.com/rust-lang/rust/issues/24616#issuecomment-112065997
// (The workaround mentioned in a later comment doesn't seem to be worth the added complexity)
impl<S> Multipart<S>
where
    S: TryStream,
    S::Ok: BodyChunk,
    S::Error: Into<ReadError>,
{
    unsafe_pinned!(inner: PushChunk<BoundaryFinder<S>, S::Ok>);
    unsafe_unpinned!(read_hdr: ReadHeaders);

    /// Construct a new `Multipart` with the given body reader and boundary.
    ///
    /// The boundary should be taken directly from the `Content-Type: multipart/form-data` header
    /// of the request. This will add the requisite `--` to the boundary as per
    /// [IETF RFC 7578 section 4.1](https://tools.ietf.org/html/rfc7578#section-4.1).
    pub fn with_body<B: Into<String>>(stream: S, boundary: B) -> Self {
        let mut boundary = boundary.into();
        boundary.insert_str(0, "--");

        // debug!("Boundary: {}", boundary);

        Multipart {
            inner: PushChunk::new(BoundaryFinder::new(stream, boundary)),
            read_hdr: ReadHeaders::default(),
        }
    }

    pub fn try_from_body_headers(body: S, headers: &HeaderMap) -> Result<Self, ReadError> {
        fn get_boundary(headers: &HeaderMap) -> Option<String> {
            Some(
                headers
                    .get(http::header::CONTENT_TYPE)?
                    .to_str()
                    .ok()?
                    .parse::<Mime>()
                    .ok()?
                    .get_param(mime::BOUNDARY)?
                    .to_string(),
            )
        }
        if let Some(boundary) = get_boundary(headers) {
            return Ok(Self::with_body(body, boundary));
        }
        Err(ReadError::Parsing("parse multiprart failed".into()))
    }

    pub fn next_field(&mut self) -> NextField<S>
    where
        Self: Unpin,
    {
        NextField::new(Pin::new(self))
    }

    /// Same as [`.next_field()`](#method.next_field) but with a receiver of `Pin<&mut Self>`.
    pub fn next_field_pinned(self: Pin<&mut Self>) -> NextField<S> {
        NextField::new(self)
    }

    /// Poll for the next boundary, returning `true` if a field should follow that boundary,
    /// or `false` if the request is at an end. See above for the overall flow.
    ///
    /// If this returns `Ready(Ok(true))`, you may then begin
    /// [polling for the headers of the next field](#method.poll_field_headers).
    ///
    /// If a field was being read, its contents will be discarded.
    ///
    /// This is a low-level call and is expected to be supplemented/replaced by a more ergonomic
    /// API once more design work has taken place.
    pub fn poll_has_next_field(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<bool, ReadError>> {
        self.as_mut().inner().stream().consume_boundary(cx)
    }

    /// Poll for the headers of the next field, returning the headers or an error otherwise.
    ///
    /// Once you have the field headers, you may then begin
    /// [polling for field chunks](#method.poll_field_chunk).
    ///
    /// In addition to bubbling up errors from the underlying stream, this will also return an
    /// error if:
    /// * the headers were corrupted, or:
    /// * did not contain a `Content-Disposition: form-data` header with a `name` parameter, or:
    /// * the end of stream was reached before the header segment terminator `\r\n\r\n`, or:
    /// * the buffer for the headers exceeds a preset size.
    ///
    /// This is a low-level call and is expected to be supplemented/replaced by a more ergonomic
    /// API once more design work has taken place.
    ///
    /// ### Note: Calling This Is Not Enforced
    /// If this step is skipped then [`.poll_field_chunk()`](#method.poll_field_chunk)
    /// will return chunks of the header segment which may or may not be desirable depending
    /// on your use-case.
    ///
    /// If you do want to inspect the raw field headers, they are separated by one CRLF (`\r\n`) and
    /// terminated by two CRLFs (`\r\n\r\n`) after which the field chunks follow.
    pub fn poll_field_headers(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<FieldHeaders, ReadError>> {
        unsafe {
            let this = self.as_mut().get_unchecked_mut();
            this.read_hdr.read_headers(Pin::new_unchecked(&mut this.inner), cx)
        }
    }

    /// Poll for the next chunk of the current field.
    ///
    /// This returns `Ready(Some(Ok(chunk)))` as long as there are chunks in the field,
    /// yielding `Ready(None)` when the next boundary is reached.
    ///
    /// You may then begin the next field with
    /// [`.poll_has_next_field()`](#method.poll_has_next_field).
    ///
    /// This is a low-level call and is expected to be supplemented/replaced by a more ergonomic
    /// API once more design work has taken place.
    ///
    /// ### Note: Call `.poll_field_headers()` First for Correct Data
    /// If [`.poll_field_headers()`](#method.poll_field_headers) is skipped then this call
    /// will return chunks of the header segment which may or may not be desirable depending
    /// on your use-case.
    ///
    /// If you do want to inspect the raw field headers, they are separated by one CRLF (`\r\n`) and
    /// terminated by two CRLFs (`\r\n\r\n`) after which the field chunks follow.
    pub fn poll_field_chunk(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<S::Ok, ReadError>>> {
        if !self.read_hdr.is_reading_headers() {
            self.inner().poll_next(cx)
        } else {
            Poll::Ready(None)
        }
    }
}

/// Struct wrapping a stream which allows a chunk to be pushed back to it to be yielded next.
pub(crate) struct PushChunk<S, T> {
    stream: S,
    pushed: Option<T>,
}

impl<S, T> PushChunk<S, T> {
    unsafe_pinned!(stream: S);
    unsafe_unpinned!(pushed: Option<T>);

    pub(crate) fn new(stream: S) -> Self {
        PushChunk { stream, pushed: None }
    }
}

impl<S: TryStream> PushChunk<S, S::Ok>
where
    S::Ok: BodyChunk,
    S::Error: Into<ReadError>,
{
    fn push_chunk(mut self: Pin<&mut Self>, chunk: S::Ok) {
        // if let Some(pushed) = self.as_mut().pushed() {
        //     debug_panic!(
        //         "pushing excess chunk: \"{}\" already pushed chunk: \"{}\"",
        //         show_bytes(chunk.as_slice()),
        //         show_bytes(pushed.as_slice())
        //     );
        // }

        debug_assert!(!chunk.is_empty(), "pushing empty chunk");

        *self.as_mut().pushed() = Some(chunk);
    }
}

impl<S: TryStream> Stream for PushChunk<S, S::Ok>
where
    S::Error: Into<ReadError>,
{
    type Item = Result<S::Ok, S::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        if let Some(pushed) = self.as_mut().pushed().take() {
            return Poll::Ready(Some(Ok(pushed)));
        }

        self.stream().try_poll_next(cx)
    }
}

#[cfg(test)]
mod test {
    use crate::http::multipart::test_util::mock_stream;
    use crate::http::multipart::FieldHeaders;

    use super::Multipart;
    // use std::convert::Infallible;

    const BOUNDARY: &str = "boundary";

    #[test]
    fn test_empty_body() {
        let multipart = Multipart::with_body(mock_stream(&[]), BOUNDARY);
        pin_mut!(multipart);
        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), false);
    }

    #[test]
    fn test_no_headers() {
        let multipart = Multipart::with_body(mock_stream(&[b"--boundary", b"\r\n", b"\r\n", b"--boundary--"]), BOUNDARY);
        pin_mut!(multipart);
        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), true);
        until_ready!(|cx| multipart.as_mut().poll_field_headers(cx)).unwrap_err();
        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), false);
    }

    #[test]
    fn test_single_field() {
        let multipart = Multipart::with_body(
            mock_stream(&[
                b"--boundary\r",
                b"\n",
                b"Content-Disposition:",
                b" form-data; name=",
                b"\"foo\"",
                b"\r\n\r\n",
                b"field data",
                b"\r",
                b"\n--boundary--",
            ]),
            BOUNDARY,
        );
        pin_mut!(multipart);

        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), true);

        ready_assert_ok_eq!(
            |cx| multipart.as_mut().poll_field_headers(cx),
            FieldHeaders {
                name: "foo".into(),
                filename: None,
                content_type: None,
                ext_headers: Default::default(),
            }
        );

        ready_assert_some_ok_eq!(|cx| multipart.as_mut().poll_field_chunk(cx), &b"field data"[..]);

        ready_assert_eq_none!(|cx| multipart.as_mut().poll_field_chunk(cx));
        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), false);
    }

    #[test]
    fn test_two_fields() {
        let multipart = Multipart::with_body(
            mock_stream(&[
                b"--boundary\r",
                b"\n",
                b"Content-Disposition:",
                b" form-data; name=",
                b"\"foo\"",
                b"\r\n\r\n",
                b"field data",
                b"\r",
                b"\n--boundary\r\n",
                b"Content-Disposition: form-data; name=",
                b"foo-",
                b"data",
                b"; filename=",
                b"\"foo.txt\"",
                b"\r\n",
                b"Content-Type: ",
                b"text/plain; charset",
                b"=utf-8",
                b"\r\n",
                b"\r\n",
                b"field data--2\r\n--data--field",
                b"\r\n--boundary--",
            ]),
            BOUNDARY,
        );
        pin_mut!(multipart);

        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), true);

        ready_assert_ok_eq!(
            |cx| multipart.as_mut().poll_field_headers(cx),
            FieldHeaders {
                name: "foo".into(),
                filename: None,
                content_type: None,
                ext_headers: Default::default(),
            }
        );

        ready_assert_some_ok_eq!(|cx| multipart.as_mut().poll_field_chunk(cx), &b"field data"[..]);
        ready_assert_eq_none!(|cx| multipart.as_mut().poll_field_chunk(cx));

        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), true);

        ready_assert_ok_eq!(
            |cx| multipart.as_mut().poll_field_headers(cx),
            FieldHeaders {
                name: "foo-data".into(),
                filename: Some("foo.txt".into()),
                content_type: Some(mime::TEXT_PLAIN_UTF_8),
                ext_headers: Default::default(),
            }
        );

        ready_assert_some_ok_eq!(|cx| multipart.as_mut().poll_field_chunk(cx), &b"field data--2\r\n--data--field"[..]);
        ready_assert_eq_none!(|cx| multipart.as_mut().poll_field_chunk(cx));

        ready_assert_ok_eq!(|cx| multipart.as_mut().poll_has_next_field(cx), false);
    }
}
