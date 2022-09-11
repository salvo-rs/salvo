use super::Piece;
use crate::http::header::{HeaderValue, LOCATION};
use crate::http::uri::Uri;
use crate::http::{Response, StatusCode};
use crate::Error;

/// Response that redirects the request to another location.
#[derive(Debug, Clone)]
pub struct Redirect {
    status_code: StatusCode,
    location: HeaderValue,
}

impl Redirect {
    /// Create a new [`Redirect`] that uses a [`303 See Other`][mdn] status code.
    ///
    /// This redirect instructs the client to change the method to GET for the subsequent request
    /// to the given `uri`, which is useful after successful form submission, file upload or when
    /// you generally don't want the redirected-to page to observe the original request method and
    /// body (if non-empty).
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid [`HeaderValue`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/303
    pub fn other(uri: impl TryInto<Uri>) -> Result<Self, Error> {
        Self::with_status_code(StatusCode::SEE_OTHER, uri)
    }

    /// Create a new [`Redirect`] that uses a [`307 Temporary Redirect`][mdn] status code.
    ///
    /// This has the same behavior as [`Redirect::to`], except it will preserve the original HTTP
    /// method and body.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid [`HeaderValue`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/307
    pub fn temporary(uri: impl TryInto<Uri>) -> Result<Self, Error> {
        Self::with_status_code(StatusCode::TEMPORARY_REDIRECT, uri)
    }

    /// Create a new [`Redirect`] that uses a [`308 Permanent Redirect`][mdn] status code.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid [`HeaderValue`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/308
    pub fn permanent(uri: impl TryInto<Uri>) -> Result<Self, Error> {
        Self::with_status_code(StatusCode::PERMANENT_REDIRECT, uri)
    }

    /// Create a new [`Redirect`] that uses a [`302 Found`][mdn] status code.
    ///
    /// This is the same as [`Redirect::temporary`], except the status code is older and thus
    /// supported by some legacy applications that doesn't understand the newer one, but some of
    /// those applications wrongly apply [`Redirect::to`] (`303 See Other`) semantics for this
    /// status code. It should be avoided where possible.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid [`HeaderValue`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/302
    pub fn found(uri: impl TryInto<Uri>) -> Result<Self, Error> {
        Self::with_status_code(StatusCode::FOUND, uri)
    }

    // This is intentionally not public since other kinds of redirects might not
    // use the `Location` header, namely `304 Not Modified`.
    //
    // We're open to adding more constructors upon request, if they make sense :)
    fn with_status_code(status_code: StatusCode, uri: impl TryInto<Uri>) -> Result<Self, Error> {
        if !status_code.is_redirection() {
            return Err(Error::other("not a redirection status code"));
        }

        Ok(Self {
            status_code,
            location: uri
                .try_into()
                .map_err(|_| Error::other("It isn't a valid URI"))
                .and_then(|uri| {
                    HeaderValue::try_from(uri.to_string()).map_err(|_| Error::other("URI isn't a valid header value"))
                })?,
        })
    }
}

impl Piece for Redirect {
    #[inline]
    fn render(self, res: &mut Response) {
        let Self { status_code, location } = self;
        res.set_status_code(status_code);
        res.headers_mut().insert(LOCATION, location);
    }
}
