use salvo::extra::serve_static::{DirHandler, Options};
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(DirHandler::width_options(
        vec!["examples/file-list/static/boy", "examples/file-list/static/girl"],
        Options {
            dot_files: false,
            listing: true,
            defaults: vec!["index.html".to_owned()],
        },
    ));
    tracing::info!("Listening on http://0.0.0.0:7878");
    Server::new(TcpListener::bind("0.0.0.0:7878")).serve(router).await;
}

#[cfg(test)]
mod tests {
    use salvo::extra::serve_static::*;
    use salvo::hyper;
    use salvo::prelude::*;

    #[tokio::test]
    async fn test_serve_static_files() {
        let router = Router::with_path("<**path>").get(DirHandler::width_options(
            vec!["static/test"],
            Options {
                dot_files: false,
                listing: true,
                defaults: vec!["index.html".to_owned()],
            },
        ));
        let service = Service::new(router);

        async fn access(service: &Service, accept: &str, url: &str) -> String {
            let req = hyper::Request::builder()
                .method("GET")
                .header("accept", accept)
                .uri(url)
                .body(hyper::Body::empty())
                .unwrap();
            service.handle(req).await.take_text().await.unwrap()
        }
        let content = access(&service, "text/plain", "http://127.0.0.1:7979/").await;
        assert!(content.contains("test1.txt") && content.contains("test2.txt"));

        let content = access(&service, "text/xml", "http://127.0.0.1:7979/").await;
        assert!(content.starts_with("<list>") && content.contains("test1.txt") && content.contains("test2.txt"));

        let content = access(&service, "text/html", "http://127.0.0.1:7979/").await;
        assert!(content.contains("<html>") && content.contains("test1.txt") && content.contains("test2.txt"));

        let content = access(&service, "application/json", "http://127.0.0.1:7979/").await;
        assert!(content.starts_with("{") && content.contains("test1.txt") && content.contains("test2.txt"));

        let content = access(&service, "text/plain", "http://127.0.0.1:7979/test1.txt").await;
        assert!(content.contains("copy1"));

        let content = access(&service, "text/plain", "http://127.0.0.1:7979/test3.txt").await;
        assert!(content.contains("Not Found"));

        let content = access(&service, "text/plain", "http://127.0.0.1:7979/../girl/love/eat.txt").await;
        assert!(content.contains("Not Found"));
        let content = access(&service, "text/plain", "http://127.0.0.1:7979/..\\girl\\love\\eat.txt").await;
        assert!(content.contains("Not Found"));

        let content = access(&service, "text/plain", "http://127.0.0.1:7979/dir1/test3.txt").await;
        assert!(content.contains("copy3"));
        let content = access(&service, "text/plain", "http://127.0.0.1:7979/dir1/dir2/test3.txt").await;
        assert!(content == "dir2 test3");
        let content = access(&service, "text/plain", "http://127.0.0.1:7979/dir1/../dir1/test3.txt").await;
        assert!(content == "copy3");
        let content = access(
            &service,
            "text/plain",
            "http://127.0.0.1:7979/dir1\\..\\dir1\\test3.txt",
        )
        .await;
        assert!(content == "copy3");
    }
}
