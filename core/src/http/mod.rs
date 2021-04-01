//some code port from https://github.com/abonander/multipart-async

pub mod body_chunk;
pub mod errors;
pub mod form;
pub mod multipart;
pub mod range;
pub mod request;
pub mod response;

pub use body_chunk::BodyChunk;
pub use cookie;
pub use http::method::Method;
pub use http::{HeaderMap, HeaderValue, StatusCode, uri, header, method, version};
pub use mime::Mime;
pub use range::HttpRange;
pub use request::Request;
pub use response::Response;
pub use errors::{HttpError, ReadError};

pub use headers;

pub fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    let dmime: Mime = default_type.unwrap_or_else(|| "text/html".parse().unwrap());
    let accept = req.accept();
    accept.first().unwrap_or(&dmime).to_string().parse().unwrap_or(dmime)
}
