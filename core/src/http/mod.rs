pub mod errors;
pub mod form;
pub mod range;
pub mod request;
pub mod response;

pub use cookie;
pub use errors::{HttpError, ReadError};
pub use http::method::Method;
pub use http::{header, method, uri, version, HeaderMap, HeaderValue, StatusCode};
pub use hyper::body::HttpBody;
pub use mime::Mime;
pub use range::HttpRange;
pub use request::Request;
pub use response::Response;

pub use headers;

pub fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    let dmime: Mime = default_type.unwrap_or_else(|| "text/html".parse().unwrap());
    let accept = req.accept();
    accept.first().unwrap_or(&dmime).to_string().parse().unwrap_or(dmime)
}

#[cfg(test)]
mod tests {
    use super::header::*;
    use super::*;
    use crate::hyper;

    #[test]
    fn test_guess_accept_mime() {
        let mut req = Request::default();
        let headers = req.headers_mut();
        headers.insert(ACCEPT, HeaderValue::from_static("application/javascript"));
        let mime = guess_accept_mime(&req, None);
        assert_eq!(mime, "application/javascript".parse::<Mime>().unwrap());
    }
    #[tokio::test]
    async fn test_query() {
        let mut request = Request::from_hyper(
            hyper::Request::builder()
                .method("GET")
                .uri("http://127.0.0.1:7979/hello?q=rust")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        assert_eq!(request.queries().len(), 1);
        assert_eq!(request.get_query::<String>("q").unwrap(), "rust");
        assert_eq!(request.get_query_or_form::<String>("q").await.unwrap(), "rust");
    }
    #[tokio::test]
    async fn test_form() {
        let mut request = Request::from_hyper(
            hyper::Request::builder()
                .method("POST")
                .header("content-type", "application/x-www-form-urlencoded")
                .uri("http://127.0.0.1:7979/hello?q=rust")
                .body("lover=dog&money=sh*t&q=firefox".into())
                .unwrap(),
        );
        assert_eq!(request.get_form::<String>("money").await.unwrap(), "sh*t");
        assert_eq!(request.get_query_or_form::<String>("q").await.unwrap(), "rust");
        assert_eq!(request.get_form_or_query::<String>("q").await.unwrap(), "firefox");

        let mut request = Request::from_hyper(
            hyper::Request::builder()
                .method("POST")
                .header(
                    "content-type",
                    "multipart/form-data; boundary=X-BOUNDARY",
                )
                .uri("http://127.0.0.1:7979/hello?q=rust")
                .body(
                    "--X-BOUNDARY\r\nContent-Disposition: form-data; \
name=\"money\"\r\n\r\nsh*t\r\n--X-BOUNDARY--\r\n"
                        .into(),
                )
                .unwrap(),
        );
        assert_eq!(request.get_form::<String>("money").await.unwrap(), "sh*t");
    }
}
