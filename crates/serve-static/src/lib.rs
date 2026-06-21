#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Serve static files and directories for Salvo web framework.
//!
//! This crate provides handlers for serving static content:
//! - `StaticDir` - Serve files from directory with options for directory listing
//! - `StaticFile` - Serve a single file
//! - `StaticEmbed` - Serve embedded files using rust-embed (when "embed" feature is enabled)
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod dir;
mod file;

pub use dir::StaticDir;
pub use file::StaticFile;
use salvo_core::cfg_feature;

#[doc(hidden)]
#[macro_export]
macro_rules! join_path {
    ($($part:expr),+) => {
        {
            let mut p = std::path::PathBuf::new();
            $(
                p.push($part);
            )*
            path_slash::PathBufExt::to_slash_lossy(&p).to_string()
        }
    }
}

cfg_feature! {
    #![feature = "embed"]
    mod embed;
    pub use embed::{render_embedded_file, static_embed, EmbeddedFileExt, StaticEmbed};
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use salvo_core::http::HeaderValue;
    use salvo_core::http::header::{CONTENT_ENCODING, VARY};
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use crate::*;

    #[cfg(unix)]
    fn create_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    #[tokio::test]
    async fn test_serve_static_dir() {
        let router = Router::with_path("{*path}").get(
            StaticDir::new(vec!["test/static"])
                .include_dot_files(false)
                .auto_list(true)
                .preload_threshold(0)
                .defaults("index.html"),
        );
        let service = Service::new(router);

        async fn access(service: &Service, accept: &str, url: &str) -> String {
            TestClient::get(url)
                .add_header("accept", accept, true)
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }
        let content = access(&service, "text/plain", "http://127.0.0.1:5801").await;
        assert!(content.contains("Index page"));
        let content = access(&service, "text/plain", "http://127.0.0.1:5801/").await;
        assert!(content.contains("Index page"));

        let content = access(&service, "text/plain", "http://127.0.0.1:5801/dir1/").await;
        assert!(content.contains("test3.txt") && content.contains("dir2"));

        let content = access(&service, "text/xml", "http://127.0.0.1:5801/dir1/").await;
        assert!(
            content.starts_with("<list>")
                && content.contains("test3.txt")
                && content.contains("dir2")
        );

        let content = access(&service, "text/html", "http://127.0.0.1:5801/dir1/").await;
        assert!(
            content.contains("<html>") && content.contains("test3.txt") && content.contains("dir2")
        );

        let content = access(&service, "application/json", "http://127.0.0.1:5801/dir1/").await;
        assert!(
            content.starts_with('{') && content.contains("test3.txt") && content.contains("dir2")
        );

        let content = access(&service, "text/plain", "http://127.0.0.1:5801/test1.txt").await;
        assert!(content.contains("copy1"));

        let content = access(&service, "text/plain", "http://127.0.0.1:5801/test3.txt").await;
        assert!(content.contains("Not Found"));

        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/../girl/love/eat.txt",
        )
        .await;
        assert!(content.contains("Not Found"));
        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/..\\girl\\love\\eat.txt",
        )
        .await;
        assert!(content.contains("Not Found"));

        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/dir1/test3.txt",
        )
        .await;
        assert!(content.contains("copy3"));
        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/dir1/dir2/test3.txt",
        )
        .await;
        assert_eq!(content, "dir2 test3");
        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/dir1/../dir1/test3.txt",
        )
        .await;
        assert_eq!(content, "copy3");
        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/dir1\\..\\dir1\\test3.txt",
        )
        .await;
        assert_eq!(content, "copy3");
    }

    #[tokio::test]
    async fn test_static_dir_rejects_symlinked_directory_escape() {
        let public = tempfile::TempDir::new().unwrap();
        let private = tempfile::TempDir::new().unwrap();
        fs::write(private.path().join("secret.txt"), "secret").unwrap();
        fs::write(private.path().join("fallback.html"), "fallback").unwrap();

        let link = public.path().join("link");
        if create_dir_symlink(private.path(), &link).is_err() {
            return;
        }

        let router = Router::with_path("{*path}")
            .get(StaticDir::new(public.path().to_path_buf()).fallback("link/fallback.html"));
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:5801/link/secret.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::NOT_FOUND);

        let response = TestClient::get("http://127.0.0.1:5801/missing")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_serve_static_file() {
        let router = Router::new()
            .push(
                Router::with_path("test1.txt").get(
                    StaticFile::new("test/static/test1.txt")
                        .chunk_size(1024)
                        .preload_threshold(0),
                ),
            )
            .push(
                Router::with_path("notexist.txt").get(StaticFile::new("test/static/notexist.txt")),
            );
        let service = Service::new(router);

        let mut response = TestClient::get("http://127.0.0.1:5801/test1.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert_eq!(response.take_string().await.unwrap(), "copy1");

        let response = TestClient::get("http://127.0.0.1:5801/notexist.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_serve_static_dir_respects_accept_encoding_q_order() {
        let router = Router::with_path("{*path}").get(StaticDir::new(vec!["test/static"]));
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:5801/test1.txt")
            .add_header("accept-encoding", "br;q=0.1, gzip;q=1", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_ENCODING),
            Some(&HeaderValue::from_static("gzip"))
        );
        let varies_on_accept_encoding = response
            .headers()
            .get_all(VARY)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|value| value.trim().eq_ignore_ascii_case("accept-encoding"));
        assert!(varies_on_accept_encoding);

        let response = TestClient::get("http://127.0.0.1:5801/test1.txt")
            .add_header("accept-encoding", "br;q=1, gzip;q=0.1", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_ENCODING),
            Some(&HeaderValue::from_static("br"))
        );

        let mut response = TestClient::get("http://127.0.0.1:5801/test1.txt")
            .add_header("accept-encoding", "identity;q=1, gzip;q=0.5", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert_eq!(response.headers().get(CONTENT_ENCODING), None);
        let varies_on_accept_encoding = response
            .headers()
            .get_all(VARY)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|value| value.trim().eq_ignore_ascii_case("accept-encoding"));
        assert!(varies_on_accept_encoding);
        assert_eq!(response.take_string().await.unwrap(), "copy1");
    }

    #[cfg(feature = "embed")]
    #[tokio::test]
    async fn test_serve_embed_files() {
        #[derive(rust_embed::RustEmbed)]
        #[folder = "test/static"]
        struct Assets;

        let router = Router::new()
            .push(
                Router::with_path("test1.txt")
                    .get(Assets::get("test1.txt").unwrap().into_handler()),
            )
            .push(Router::with_path("files/{**path}").get(serve_file))
            .push(
                Router::with_path("dir/{**path}").get(
                    static_embed::<Assets>()
                        .defaults("index.html")
                        .fallback("fallback.html"),
                ),
            )
            .push(Router::with_path("dir2/{**path}").get(static_embed::<Assets>()))
            .push(
                Router::with_path("dir3/{**path}")
                    .get(static_embed::<Assets>().fallback("notexist.html")),
            );
        let service = Service::new(router);

        #[handler]
        async fn serve_file(req: &mut Request, res: &mut Response) {
            let path = req.param::<String>("path").unwrap();
            if let Some(file) = Assets::get(&path) {
                file.render(req, res);
            }
        }

        let mut response = TestClient::get("http://127.0.0.1:5801/files/test1.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert_eq!(response.take_string().await.unwrap(), "copy1");

        let mut response = TestClient::get("http://127.0.0.1:5801/dir/test1.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert_eq!(response.take_string().await.unwrap(), "copy1");

        let mut response = TestClient::get("http://127.0.0.1:5801/dir/test1111.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert!(
            response
                .take_string()
                .await
                .unwrap()
                .contains("Fallback page")
        );

        let response = TestClient::get("http://127.0.0.1:5801/dir")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);

        let mut response = TestClient::get("http://127.0.0.1:5801/dir/")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        assert!(response.take_string().await.unwrap().contains("Index page"));

        let response = TestClient::get("http://127.0.0.1:5801/dir2/")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::NOT_FOUND);

        let response = TestClient::get("http://127.0.0.1:5801/dir3/abc.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::NOT_FOUND);

        // A 200 response carries a quoted RFC 7232 ETag...
        let response = TestClient::get("http://127.0.0.1:5801/files/test1.txt")
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::OK);
        let etag = response
            .headers
            .get("etag")
            .expect("200 response should carry an ETag")
            .to_str()
            .unwrap()
            .to_owned();
        assert!(etag.starts_with('"') && etag.ends_with('"'));

        // ...and echoing it back yields a 304 that still carries the same
        // validator (RFC 7232 §4.1), not a bare 304.
        let response = TestClient::get("http://127.0.0.1:5801/files/test1.txt")
            .add_header("if-none-match", &etag, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            response.headers.get("etag").and_then(|v| v.to_str().ok()),
            Some(etag.as_str())
        );
    }
}
