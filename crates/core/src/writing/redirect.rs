use super::Scribe;
use crate::Error;
use crate::http::header::{HeaderValue, LOCATION};
use crate::http::uri::Uri;
use crate::http::{Response, StatusCode};

/// Response that redirects the request to another location.
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn hello(res: &mut Response) {
///     res.render(Redirect::found("https://www.rust-lang.org/"))
/// }
/// ```
#[derive(Clone, Debug)]
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
    /// If `uri` isn't a valid [`Uri`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/303
    pub fn other(uri: impl TryInto<Uri>) -> Self {
        Self::with_status_code(StatusCode::SEE_OTHER, uri).expect("invalid uri")
    }

    /// Create a new [`Redirect`] that uses a [`307 Temporary Redirect`][mdn] status code.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid [`Uri`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/307
    pub fn temporary(uri: impl TryInto<Uri>) -> Self {
        Self::with_status_code(StatusCode::TEMPORARY_REDIRECT, uri).expect("invalid uri")
    }

    /// Create a new [`Redirect`] that uses a [`308 Permanent Redirect`][mdn] status code.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid [`Uri`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/308
    pub fn permanent(uri: impl TryInto<Uri>) -> Self {
        Self::with_status_code(StatusCode::PERMANENT_REDIRECT, uri).expect("invalid uri")
    }

    /// Create a new [`Redirect`] that uses a [`302 Found`][mdn] status code.
    ///
    /// This is the same as [`Redirect::temporary`], except the status code is older and thus
    /// supported by some legacy applications that doesn't understand the newer one, but some of
    /// those applications wrongly apply [`Redirect::other`] (`303 See Other`) semantics for this
    /// status code. It should be avoided where possible.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid [`Uri`].
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/302
    pub fn found(uri: impl TryInto<Uri>) -> Self {
        Self::with_status_code(StatusCode::FOUND, uri).expect("invalid uri")
    }

    /// Create a new [`Redirect`] that uses a status code.
    pub fn with_status_code(
        status_code: StatusCode,
        uri: impl TryInto<Uri>,
    ) -> Result<Self, Error> {
        if !status_code.is_redirection() {
            return Err(Error::other("not a redirection status code"));
        }

        Ok(Self {
            status_code,
            location: uri
                .try_into()
                .map_err(|_| Error::other("It isn't a valid URI"))
                .and_then(|uri| {
                    HeaderValue::try_from(uri.to_string())
                        .map_err(|_| Error::other("URI isn't a valid header value"))
                })?,
        })
    }
}

impl Scribe for Redirect {
    #[inline]
    fn render(self, res: &mut Response) {
        let Self {
            status_code,
            location,
        } = self;
        res.status_code(status_code);
        res.headers_mut().insert(LOCATION, location);
    }
}
