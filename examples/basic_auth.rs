use salvo::routing::Router;
use salvo::Server;
use salvo::extra::basic_auth::{BasicAuthConfig, BasicAuthHandler};
use salvo::extra::serve::StaticDir;

#[tokio::main]
async fn main() {
    let baconfig = BasicAuthConfig {
        realm: "realm".to_owned(),
        context_key: Some("user_name".to_owned()),
        validator: Box::new(|user_name, password| -> bool { user_name == "root" && password == "pwd" }),
    };
    let auth_handler = BasicAuthHandler::new(baconfig);

    let router = Router::new()
        .before(auth_handler)
        .get(StaticDir::new(vec!["examples/static/body", "examples/static/girl"]));
    Server::new(router).bind(([0, 0, 0, 0], 7879)).await;
}
