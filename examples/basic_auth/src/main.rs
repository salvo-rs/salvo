use novel::{Server};
use novel::logging;
use novel::routing::{Router};
use novel_extra::serve::Static;
use novel_extra::auth::{BasicAuthHandler, BasicAuthConfig};
use novel::routing::Method;
use hyper;
#[macro_use]
extern crate slog;

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
    let addr = "127.0.0.1:9689";
    info!(logging::logger(), "Server running"; "address" => addr);
    hyper::rt::run(server.serve(addr));
}