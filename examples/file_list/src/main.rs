use novel::logging;
use novel::{Server};
use novel::routing::{Router};
use novel_extra::serve::{Static};
use hyper;
#[macro_use]
extern crate slog;

fn main() {
    let mut router = Router::new("/<*path>");
    router.get(Static::from("./static/root1"));
    let server = Server::new(router);
    let addr = "127.0.0.1:9688";
    info!(logging::logger(), "Server running"; "address" => addr);
    hyper::rt::run(server.serve("127.0.0.1:9688"));
}