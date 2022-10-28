use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}
#[handler]
async fn hello1() -> Result<&'static str, ()> {
    Ok("Hello World1")
}
#[handler]
async fn hello2(res: &mut Response) {
    res.render("Hello World2");
}
#[handler]
async fn hello3(_req: &mut Request, res: &mut Response) {
    res.render(Text::Plain("Hello World3"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(route()).await;
}

fn route() -> Router {
    Router::new()
        .get(hello)
        .push(Router::with_path("hello1").get(hello1))
        .push(Router::with_path("hello2").get(hello2))
        .push(Router::with_path("hello3").get(hello3))
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_hello() {
        let service = Service::new(super::route());

        async fn access(service: &Service, name: &str) -> String {
            TestClient::get(format!("http://127.0.0.1:7878/{}", name))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert_eq!(access(&service, "hello1").await, "Hello World1");
        assert_eq!(access(&service, "hello2").await, "Hello World2");
        assert_eq!(access(&service, "hello3").await, "Hello World3");
    }
}
