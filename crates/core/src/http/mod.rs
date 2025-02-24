//! The HTTP related types and functions.

pub mod errors;
pub mod form;
mod range;
pub mod request;
pub mod response;
cfg_feature! {
    #![feature = "cookie"]
    pub use cookie;
}
pub use errors::{ParseError, ParseResult, StatusError, StatusResult};
pub use headers;
pub use http::method::Method;
pub use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header, method, uri};
pub use mime::{self, Mime};
pub use range::HttpRange;
pub use request::Request;
pub mod body;
pub use body::{Body, ReqBody, ResBody};
pub use response::Response;

pub use http::version::Version;

use std::io::Result as IoResult;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::conn::HttpBuilder;
use crate::fuse::ArcFusewire;
use crate::service::HyperHandler;

/// A helper trait for http connection.
pub trait HttpConnection {
    /// Serve this http connection.
    fn serve(
        self,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> impl Future<Output = IoResult<()>> + Send;

    /// Get the fusewire of this connection.
    fn fusewire(&self) -> Option<ArcFusewire>;
}

// /// Get Http version from alph.
// pub fn version_from_alpn(proto: impl AsRef<[u8]>) -> Version {
//     if proto.as_ref().windows(2).any(|window| window == b"h2") {
//         Version::HTTP_2
//     } else {
//         Version::HTTP_11
//     }
// }

#[doc(hidden)]
pub fn parse_accept_encoding(header: &str) -> Vec<(String, u8)> {
    let mut vec = header
        .split(',')
        .filter_map(|s| {
            let mut iter = s.trim().split(';');
            let (algo, q) = (iter.next()?, iter.next());
            let algo = algo.trim();
            let q = q
                .and_then(|q| {
                    q.trim()
                        .strip_prefix("q=")
                        .and_then(|q| q.parse::<f32>().map(|f| (f * 100.0) as u8).ok())
                })
                .unwrap_or(100u8);
            Some((algo.to_owned(), q))
        })
        .collect::<Vec<(String, u8)>>();

    vec.sort_by(|(_, a), (_, b)| match b.cmp(a) {
        std::cmp::Ordering::Equal => std::cmp::Ordering::Greater,
        other => other,
    });

    vec
}

#[doc(hidden)]
#[inline]
pub fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    let dmime: Mime = default_type.unwrap_or(mime::TEXT_HTML);
    let accept = req.accept();
    accept
        .first()
        .unwrap_or(&dmime)
        .to_string()
        .parse()
        .unwrap_or(dmime)
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
