use salvo::extra::basic_auth::BasicAuthHandler;
use salvo::extra::serve::StaticDir;
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let auth_handler = BasicAuthHandler::new(validate);

    let router = Router::new()
        .hoop(auth_handler)
        .get(StaticDir::new(vec!["examples/static/boy", "examples/static/girl"]));
    Server::new(TcpListener::bind("0.0.0.0:7878")).serve(router).await;
}

async fn validate(username: &str, password: &str) -> bool {
    username == "root" && password == "pwd"
}
