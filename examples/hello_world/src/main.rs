use novel::logging;
use hyper;
#[macro_use]
extern crate slog;
use novel::{Server, Context};
use novel::routing::{Router};

fn hello_world(ctx: &mut Context) {
    ctx.render_text("Hello World");
}
fn hello_world2(ctx: &mut Context) {
    ctx.render_text("Hello World2");
}
fn main() {
    let mut router = Router::new("/");
    router.get(hello_world);
    router.minion("hello2").get(hello_world2);
    let server = Server::new(router);
    let addr = "127.0.0.1:9189";
    info!(logging::logger(), "Server running"; "address" => addr);
    hyper::rt::run(server.serve(addr));
}