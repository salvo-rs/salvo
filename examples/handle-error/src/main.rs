use salvo::anyhow;
use salvo::prelude::*;

struct CustomError;
#[async_trait]
impl Writer for CustomError {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render("custom error");
    }
}

#[fn_handler]
async fn handle_anyhow() -> Result<(), anyhow::Error> {
    Err(anyhow::anyhow!("anyhow error"))
}
#[fn_handler]
async fn handle_custom() -> Result<(), CustomError> {
    Err(CustomError)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://0.0.0.0:7878");
    Server::new(TcpListener::bind("0.0.0.0:7878")).serve(route()).await;
}

fn route() -> Router {
    Router::new()
        .push(Router::with_path("anyhow").get(handle_anyhow))
        .push(Router::with_path("custom").get(handle_custom))
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_handle_error() {
        use salvo::hyper;
        use salvo::prelude::*;

        let service = Service::new(super::route());

        async fn access(service: &Service, name: &str) -> String {
            let req = hyper::Request::builder()
                .method("GET")
                .uri(format!("http://127.0.0.1:7878/{}", name));
            let req: Request = req.body(hyper::Body::empty()).unwrap().into();
            service.handle(req).await.take_text().await.unwrap()
        }

        assert!(access(&service, "anyhow").await.contains("500: Internal Server Error"));
        assert_eq!(access(&service, "custom").await, "custom error");
    }
}
