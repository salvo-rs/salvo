use salvo::{Server};
use salvo::routing::{Router};
use salvo_extra::serve::Static;
use salvo_extra::auth::basic::{BasicAuthHandler, BasicAuthConfig};
use salvo::routing::Method;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let baconfig = BasicAuthConfig{
        realm: "realm".to_owned(),
        context_key: Some("user_name".to_owned()),
        expires: None,
        validator: Box::new(|user_name, password|->bool{
            user_name == "root" && password == "pwd"
        }),
    };
    let auth_handler = BasicAuthHandler::new(baconfig);

    let mut router = Router::new("/<*path>");
    router.before(Method::ALL, auth_handler);
    router.get(Static::from("./static/root1"));
    let server = Server::new(router);
    server.serve().await?;
    Ok(())
}