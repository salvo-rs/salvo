use salvo::prelude::*;
use salvo::Catcher;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878"))
        .serve(create_service())
        .await;
}

fn create_service() -> Service {
    let router = Router::new().get(hello_world);
    let catchers: Vec<Box<dyn Catcher>> = vec![Box::new(Handle404)];
    Service::new(router).with_catchers(catchers)
}

struct Handle404;
impl Catcher for Handle404 {
    fn catch(&self, _req: &Request, _depot: &Depot, res: &mut Response) -> bool {
        if let Some(StatusCode::NOT_FOUND) = res.status_code() {
            res.render("Custom 404 Error Page");
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_handle_error() {
        use salvo::prelude::*;
        use salvo::test::{ResponseExt, TestClient};

        let service = super::create_service();

        async fn access(service: &Service, name: &str) -> String {
            TestClient::get(format!("http://127.0.0.1:7878/{}", name))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert_eq!(access(&service, "notfound").await, "Custom 404 Error Page");
    }
}
