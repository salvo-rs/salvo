//! Http module

pub mod errors;
pub mod form;
mod range;
pub mod request;
pub mod response;
cfg_feature! {
    #![feature = "cookie"]
    pub use cookie;
}
pub use errors::{ParseError, StatusError};
pub use headers;
pub use http::method::Method;
pub use http::{header, method, uri, HeaderMap, HeaderValue, StatusCode};
pub use mime::{self, Mime};
pub use range::HttpRange;
pub use request::Request;
pub mod body;
pub use body::{Body, ReqBody, ResBody};
pub use response::Response;

pub use http::version::Version;

use std::io::Result as IoResult;
use std::sync::Arc;

use crate::async_trait;
use crate::conn::HttpBuilders;
use crate::service::HyperHandler;

/// A helper trait for get a protocol from certain types.
#[async_trait]
pub trait HttpConnection {
    /// The http protocol version.
    async fn version(&mut self) -> Option<Version>;
    /// Serve this http connection.
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()>;
}

/// Get Http version from alph.
pub fn version_from_alpn(proto: impl AsRef<[u8]>) -> Version {
    if proto.as_ref().windows(2).any(|window| window == b"h2") {
        Version::HTTP_2
    } else {
        Version::HTTP_11
    }
}

#[inline]
pub(crate) fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    let dmime: Mime = default_type.unwrap_or_else(|| "text/html".parse().unwrap());
    let accept = req.accept();
    accept.first().unwrap_or(&dmime).to_string().parse().unwrap_or(dmime)
}

#[cfg(test)]
mod tests {
    use super::header::*;
    use super::*;

    #[test]
    fn test_guess_accept_mime() {
        let mut req = Request::default();
        let headers = req.headers_mut();
        headers.insert(ACCEPT, HeaderValue::from_static("application/javascript"));
        let mime = guess_accept_mime(&req, None);
        assert_eq!(mime, "application/javascript".parse::<Mime>().unwrap());
    }
}
