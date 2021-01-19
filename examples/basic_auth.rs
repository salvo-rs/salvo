use salvo::routing::Router;
use salvo::Server;
use salvo_extra::auth::basic::{BasicAuthConfig, BasicAuthHandler};
use salvo_extra::serve::Static;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let baconfig = BasicAuthConfig {
        realm: "realm".to_owned(),
        context_key: Some("user_name".to_owned()),
        expires: None,
        validator: Box::new(|user_name, password| -> bool { user_name == "root" && password == "pwd" }),
    };
    let auth_handler = BasicAuthHandler::new(baconfig);

    let router = Router::new().before(auth_handler).get(Static::from("./static/root1"));
    Server::new(router).bind(([127, 0, 0, 1], 7879)).await;
    Ok(())
}
