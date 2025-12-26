use salvo::macros::Extractible;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Extractible, Serialize, Deserialize)]
#[salvo(extract(default_source(from = "query")))]
struct Page {
    #[serde(default)]
    page: usize,
    #[serde(alias = "name")]
    #[serde(alias = "location")]
    country: String,
}

#[handler]
async fn index(req: &mut Request) -> String {
    let page: Page = req.extract().await.unwrap();
    format!("{page:?}")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(index);

    println!("Example URL: http://0.0.0.0:8698/?page=1&name=france");
    println!("Example URL: http://0.0.0.0:8698/?page=2&location=italy");
    println!("Example URL: http://0.0.0.0:8698/?page=3&country=spain");
    
    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};
    use super::*;

    #[tokio::test]
    async fn test_extract_alias() {
        let service = Service::new(Router::new().get(index));

        let content = TestClient::get("http://127.0.0.1:8698/?page=1&name=france")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains(r#"country: "france""#));

        let content = TestClient::get("http://127.0.0.1:8698/?page=2&location=italy")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains(r#"country: "italy""#));

        let content = TestClient::get("http://127.0.0.1:8698/?page=3&country=spain")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains(r#"country: "spain""#));
    }
}
