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
