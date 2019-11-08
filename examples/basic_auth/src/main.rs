use novel::{Server};
use novel::routing::{Router};
use novel_extra::serve::Static;
use novel_extra::auth::{BasicAuthHandler, BasicAuthConfig};
use novel::routing::Method;
use hyper;

fn main() {
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
    hyper::rt::run(server.serve());
}