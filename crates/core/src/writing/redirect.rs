use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use super::Scribe;
use crate::Error;
use crate::http::header::{HeaderValue, LOCATION};
use crate::http::{Response, StatusCode};

/// Characters that should NOT be percent-encoded in URI paths.
/// This includes unreserved characters (RFC 3986 §2.3) plus common path delimiters.
const PATH_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'/')
    .remove(b':')
    .remove(b'@')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b'%')
    .remove(b'?')
    .remove(b'#')
    .remove(b'[')
    .remove(b']');

/// Response that redirects the request to another location.
///
/// The redirect URL is automatically percent-encoded if it contains non-ASCII
/// characters (e.g., Chinese characters, emoji), so both pre-encoded and
/// unencoded URLs are accepted.
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
///
/// Unicode paths also work:
///
/// ```
/// use salvo_core::writing::Redirect;
///
/// // Chinese characters are automatically percent-encoded
/// let redirect = Redirect::found("/path/汉字");
/// // Already-encoded URLs pass through unchanged
/// let redirect = Redirect::found("/path/%E6%B1%89%E5%AD%97");
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
    /// If `uri` isn't a valid header value after percent-encoding.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/303
    pub fn other(uri: impl AsRef<str>) -> Self {
        Self::with_status_code(StatusCode::SEE_OTHER, uri).expect("invalid uri")
    }

    /// Create a new [`Redirect`] that uses a [`307 Temporary Redirect`][mdn] status code.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid header value after percent-encoding.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/307
    pub fn temporary(uri: impl AsRef<str>) -> Self {
        Self::with_status_code(StatusCode::TEMPORARY_REDIRECT, uri).expect("invalid uri")
    }

    /// Create a new [`Redirect`] that uses a [`308 Permanent Redirect`][mdn] status code.
    ///
    /// # Panics
    ///
    /// If `uri` isn't a valid header value after percent-encoding.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/308
    pub fn permanent(uri: impl AsRef<str>) -> Self {
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
    /// If `uri` isn't a valid header value after percent-encoding.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/302
    pub fn found(uri: impl AsRef<str>) -> Self {
        Self::with_status_code(StatusCode::FOUND, uri).expect("invalid uri")
    }

    /// Create a new [`Redirect`] that uses the given status code.
    ///
    /// Non-ASCII characters in the URI are automatically percent-encoded.
    pub fn with_status_code(status_code: StatusCode, uri: impl AsRef<str>) -> Result<Self, Error> {
        if !status_code.is_redirection() {
            return Err(Error::other("not a redirection status code"));
        }

        let encoded = encode_uri(uri.as_ref());
        let location = HeaderValue::try_from(encoded)
            .map_err(|_| Error::other("URI isn't a valid header value"))?;

        Ok(Self {
            status_code,
            location,
        })
    }
}

/// Percent-encode non-ASCII characters in a URI string.
///
/// Already-encoded sequences (`%XX`) are preserved.
fn encode_uri(uri: &str) -> String {
    utf8_percent_encode(uri, PATH_ENCODE_SET).to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redirect_ascii_url() {
        let r = Redirect::found("/hello/world");
        assert_eq!(r.location.to_str().unwrap(), "/hello/world");
    }

    #[test]
    fn test_redirect_chinese_characters() {
        let r = Redirect::found("/1235/汉字");
        assert_eq!(r.location.to_str().unwrap(), "/1235/%E6%B1%89%E5%AD%97");
    }

    #[test]
    fn test_redirect_already_encoded() {
        let r = Redirect::found("/1235/%E6%B1%89%E5%AD%97");
        assert_eq!(r.location.to_str().unwrap(), "/1235/%E6%B1%89%E5%AD%97");
    }

    #[test]
    fn test_redirect_full_url_with_unicode() {
        let r = Redirect::found("https://example.com/路径/文件");
        assert_eq!(
            r.location.to_str().unwrap(),
            "https://example.com/%E8%B7%AF%E5%BE%84/%E6%96%87%E4%BB%B6"
        );
    }

    #[test]
    fn test_redirect_preserves_query_and_fragment() {
        let r = Redirect::found("/search?q=日本語&lang=ja#結果");
        let loc = r.location.to_str().unwrap();
        assert!(loc.starts_with("/search?q="));
        assert!(loc.contains("&lang=ja"));
        assert!(loc.contains("#"));
    }

    #[test]
    fn test_redirect_emoji() {
        let r = Redirect::found("/emoji/🦀");
        let loc = r.location.to_str().unwrap();
        assert!(loc.starts_with("/emoji/"));
        assert!(!loc.contains('🦀'));
        assert!(loc.contains("%F0%9F%A6%80"));
    }

    #[test]
    fn test_redirect_status_codes() {
        let r = Redirect::other("/a");
        assert_eq!(r.status_code, StatusCode::SEE_OTHER);

        let r = Redirect::temporary("/a");
        assert_eq!(r.status_code, StatusCode::TEMPORARY_REDIRECT);

        let r = Redirect::permanent("/a");
        assert_eq!(r.status_code, StatusCode::PERMANENT_REDIRECT);

        let r = Redirect::found("/a");
        assert_eq!(r.status_code, StatusCode::FOUND);
    }

    #[test]
    fn test_redirect_invalid_status_code() {
        let result = Redirect::with_status_code(StatusCode::OK, "/test");
        assert!(result.is_err());
    }
}
