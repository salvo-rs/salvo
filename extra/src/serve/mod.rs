mod dir;
mod fs;

pub use dir::{Options, StaticDir};
pub use fs::StaticFile;

#[cfg(test)]
mod tests {
    use salvo_core::hyper;
    use salvo_core::prelude::*;

    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct JwtClaims {
        user: String,
        exp: i64,
    }
    #[tokio::test]
    async fn test_serve_static_files() {
        let router = Router::with_path("<**path>").get(StaticDir::width_options(
            vec!["../examples/static/test"],
            Options {
                dot_files: false,
                listing: true,
                defaults: vec!["index.html".to_owned()],
            },
        ));
        let service = Service::new(router);

        async fn access(service: &Service, url: &str) -> String {
            let request = Request::from_hyper(
                hyper::Request::builder()
                    .method("GET")
                    .uri(url)
                    .body(hyper::Body::empty())
                    .unwrap(),
            );
            service.handle(request).await.take_text().await.unwrap()
        }
        let content = access(&service, "http://127.0.0.1:7979/").await;
        assert!(content.contains("test1.txt") && content.contains("test2.txt"));

        let content = access(&service, "http://127.0.0.1:7979/test1.txt").await;
        assert!(content.contains("copy1"));

        let content = access(&service, "http://127.0.0.1:7979/test3.txt").await;
        assert!(content.contains("Not Found"));
    }
}
