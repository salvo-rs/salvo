use salvo::extra::basic_auth::{BasicAuthHandler, BasicAuthValidator};
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://0.0.0.0:7878");
    Server::new(TcpListener::bind("0.0.0.0:7878"))
        .serve(route())
        .await;
}
fn route() -> Router {
    let auth_handler = BasicAuthHandler::new(Validator);
    Router::with_hoop(auth_handler).handle(hello)
}
#[fn_handler]
async fn hello() -> &'static str {
    "Hello"
}

struct Validator;
#[async_trait]
impl BasicAuthValidator for Validator {
    async fn validate(&self, username: &str, password: &str) -> bool {
        username == "root" && password == "pwd"
    }
}

#[cfg(test)]
mod tests {
    use salvo::hyper;
    use salvo::prelude::*;
    use salvo::http::headers::{Authorization, HeaderMapExt};

    #[tokio::test]
    async fn test_basic_auth() {

        let service = Service::new(super::route());

        let mut req = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7878/");
        let headers = req.headers_mut().unwrap();
        headers.typed_insert(Authorization::basic("root", "pwd"));
        let req: Request = req.body(hyper::Body::empty()).unwrap().into();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert!(content.contains("Hello"));

        let mut req = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7878/");
        let headers = req.headers_mut().unwrap();
        headers.typed_insert(Authorization::basic("root", "pwd2"));
        let req: Request = req.body(hyper::Body::empty()).unwrap().into();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert!(content.contains("Unauthorized"));
    }
}
