use salvo::extra::basic_auth::BasicAuthHandler;
use salvo::extra::serve::StaticDir;
use salvo::routing::Router;
use salvo::Server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let validator = |user_name, password| -> bool { user_name == "root" && password == "pwd" };
    let auth_handler = BasicAuthHandler::new(validator);

    let router = Router::new()
        .hoop(auth_handler)
        .get(StaticDir::new(vec!["examples/static/boy", "examples/static/girl"]));
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
