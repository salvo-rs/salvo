use salvo::prelude::*;

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}
#[handler]
async fn hello_world1() -> Result<&'static str, ()> {
    Ok("Hello World1")
}
#[handler]
async fn hello_world2(res: &mut Response) {
    res.render("Hello World2");
}
#[handler]
async fn hello_world3(_req: &mut Request, res: &mut Response) {
    res.render(Text::Plain("Hello World3"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    match TcpListener::try_bind("127.0.0.1:7878") {
        Ok(listener) => Server::new(listener).serve(route()).await,
        Err(e) => tracing::error!(error = ?e, "ddd")
    } 
}

fn route() -> Router {
    Router::new()
        .get(hello_world)
        .push(Router::with_path("hello1").get(hello_world1))
        .push(Router::with_path("hello2").get(hello_world2))
        .push(Router::with_path("hello3").get(hello_world3))
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_hello_world() {
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
