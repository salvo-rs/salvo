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
    use cookie::Cookie;
    use serde::Deserialize;

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
                    "multipart/form-data; boundary=----WebKitFormBoundary0mkL0yrNNupCojyz",
                )
                .uri("http://127.0.0.1:7979/hello?q=rust")
                .body(
                    "------WebKitFormBoundary0mkL0yrNNupCojyz\r\n\
Content-Disposition: form-data; name=\"money\"\r\n\r\nsh*t\r\n\
------WebKitFormBoundary0mkL0yrNNupCojyz\r\n\
Content-Disposition: form-data; name=\"file1\"; filename=\"err.txt\"\r\n\
Content-Type: text/plain\r\n\r\n\
file content\r\n\
------WebKitFormBoundary0mkL0yrNNupCojyz--\r\n"
                        .into(),
                )
                .unwrap(),
        );
        assert_eq!(request.get_form::<String>("money").await.unwrap(), "sh*t");
        let file = request.get_file("file1").await.unwrap();
        assert_eq!(file.file_name().unwrap(), "err.txt");
        let files = request.get_files("file1").await.unwrap();
        assert_eq!(files[0].file_name().unwrap(), "err.txt");
    }

    #[tokio::test]
    async fn test_response() {
        let mut response = Response::from_hyper(
            hyper::Response::builder()
                .header("set-cookie", "lover=dog")
                .body("response body".into())
                .unwrap(),
        );
        assert_eq!(response.header_cookies().len(), 1);
        response.cookies_mut().add(Cookie::new("money", "sh*t"));
        assert_eq!(response.cookies().get("money").unwrap().value(), "sh*t");
        response.commit();
        assert_eq!(response.header_cookies().len(), 2);
        assert_eq!(response.take_bytes().await.unwrap().len(), b"response body".len());

        #[derive(Deserialize, Eq, PartialEq, Debug)]
        struct User {
            name: String,
        }

        let mut response = Response::from_hyper(
            hyper::Response::builder()
                .body(r#"{"name": "jobs"}"#.into())
                .unwrap(),
        );
        assert_eq!(response.take_json::<User>().await.unwrap(), User {name: "jobs".into()});
    }
}
