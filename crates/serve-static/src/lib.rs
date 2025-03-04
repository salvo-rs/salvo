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

use percent_encoding::{CONTROLS, utf8_percent_encode};
use salvo_core::Response;
use salvo_core::http::uri::{Parts as UriParts, Uri};
use salvo_core::writing::Redirect;

pub use dir::StaticDir;
pub use file::StaticFile;

#[macro_use]
mod cfg;

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

#[inline]
pub(crate) fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, CONTROLS).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[inline]
pub(crate) fn decode_url_path_safely(path: &str) -> String {
    percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .to_string()
}

#[inline]
pub(crate) fn format_url_path_safely(path: &str) -> String {
    let final_slash = if path.ends_with('/') { "/" } else { "" };
    let mut used_parts = Vec::with_capacity(8);
    for part in path.split(['/', '\\']) {
        if part.is_empty() || part == "." || (cfg!(windows) && part.contains(':')) {
            continue;
        } else if part == ".." {
            used_parts.pop();
        } else {
            used_parts.push(part);
        }
    }
    used_parts.join("/") + final_slash
}

pub(crate) fn redirect_to_dir_url(req_uri: &Uri, res: &mut Response) {
    let UriParts {
        scheme,
        authority,
        path_and_query,
        ..
    } = req_uri.clone().into_parts();
    let mut builder = Uri::builder();
    if let Some(scheme) = scheme {
        builder = builder.scheme(scheme);
    }
    if let Some(authority) = authority {
        builder = builder.authority(authority);
    }
    if let Some(path_and_query) = path_and_query {
        if let Some(query) = path_and_query.query() {
            builder = builder.path_and_query(format!("{}/?{}", path_and_query.path(), query));
        } else {
            builder = builder.path_and_query(format!("{}/", path_and_query.path()));
        }
    }
    let redirect_uri = builder.build().expect("Invalid uri");
    res.render(Redirect::found(redirect_uri));
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use crate::*;

    #[tokio::test]
    async fn test_serve_static_dir() {
        let router = Router::with_path("{*path}").get(
            StaticDir::new(vec!["test/static"])
                .include_dot_files(false)
                .auto_list(true)
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
        assert!(content == "dir2 test3");
        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/dir1/../dir1/test3.txt",
        )
        .await;
        assert!(content == "copy3");
        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:5801/dir1\\..\\dir1\\test3.txt",
        )
        .await;
        assert!(content == "copy3");
    }

    #[tokio::test]
    async fn test_serve_static_file() {
        let router = Router::new()
            .push(
                Router::with_path("test1.txt")
                    .get(StaticFile::new("test/static/test1.txt").chunk_size(1024)),
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
    }
}
