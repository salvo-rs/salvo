use salvo::extra::basic_auth::{BasicAuthHandler, BasicAuthValidator};
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let auth_handler = BasicAuthHandler::new(Validator);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878"))
        .serve(Router::with_hoop(auth_handler).handle(hello))
        .await;
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
