use novel::{Server};
use novel::routing::{Router};
use novel_extra::serve::{Static};
use hyper;

fn main() {
    let mut router = Router::new("/<*path>");
    router.get(Static::from("./static/root1"));
    let server = Server::with_addr(router, "127.0.0.1:9688");
    hyper::rt::run(server.serve());
}