use super::preflight_request_headers;
use salvo_core::http::{HeaderValue, header::HeaderName};

/// Holds configuration for how to set the [`Vary`][mdn] header.
///
/// See [`Cors::vary`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Vary
/// [`Cors::vary`]: super::Cors::vary
#[derive(Clone, Debug)]
pub struct Vary(Vec<HeaderValue>);

impl Vary {
    /// Set the list of header names to return as vary header values
    ///
    /// See [`Cors::vary`] for more details.
    ///
    /// [`Cors::vary`]: super::Cors::vary
    pub fn list<I>(headers: I) -> Self
    where
        I: IntoIterator<Item = HeaderName>,
    {
        Self(headers.into_iter().map(Into::into).collect())
    }

    pub(super) fn values(&self) -> impl Iterator<Item = HeaderValue> + '_ {
        self.0.iter().cloned()
    }
}

impl Default for Vary {
    fn default() -> Self {
        Self::list(preflight_request_headers())
    }
}

impl<const N: usize> From<[HeaderName; N]> for Vary {
    fn from(arr: [HeaderName; N]) -> Self {
        Self::list(arr)
    }
}

impl From<Vec<HeaderName>> for Vary {
    fn from(vec: Vec<HeaderName>) -> Self {
        Self::list(vec)
    }
}
#[cfg(test)]
mod tests {
    use salvo_core::http::header::{
        self, ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, HeaderName,
        HeaderValue, ORIGIN,
    };

    use super::{Vary, preflight_request_headers};

    #[test]
    fn test_default() {
        let vary = Vary::default();
        let headers: Vec<HeaderValue> = vary.values().collect();
        let expected: Vec<HeaderValue> = preflight_request_headers().map(Into::into).collect();
        assert_eq!(headers, expected);
    }

    #[test]
    fn test_list() {
        let vary = Vary::list(vec![header::ACCEPT, header::ACCEPT_LANGUAGE]);
        let headers: Vec<HeaderValue> = vary.values().collect();
        assert_eq!(
            headers,
            vec![
                HeaderValue::from_static("accept"),
                HeaderValue::from_static("accept-language")
            ]
        );
    }

    #[test]
    fn test_from_array() {
        let vary: Vary = [header::ACCEPT, header::ACCEPT_LANGUAGE].into();
        let headers: Vec<HeaderValue> = vary.values().collect();
        assert_eq!(
            headers,
            vec![
                HeaderValue::from_static("accept"),
                HeaderValue::from_static("accept-language")
            ]
        );
    }

    #[test]
    fn test_from_vec() {
        let vary: Vary = vec![header::ACCEPT, header::ACCEPT_LANGUAGE].into();
        let headers: Vec<HeaderValue> = vary.values().collect();
        assert_eq!(
            headers,
            vec![
                HeaderValue::from_static("accept"),
                HeaderValue::from_static("accept-language")
            ]
        );
    }

    #[test]
    fn test_preflight_request_headers() {
        let mut headers = preflight_request_headers().map(HeaderName::from);
        assert_eq!(headers.next(), Some(ORIGIN));
        assert_eq!(headers.next(), Some(ACCESS_CONTROL_REQUEST_METHOD));
        assert_eq!(headers.next(), Some(ACCESS_CONTROL_REQUEST_HEADERS));
        assert_eq!(headers.next(), None);
    }
}
