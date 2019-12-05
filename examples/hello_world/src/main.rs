use hyper;
use novel::{Server, Context};
use novel::routing::{Router};

fn hello_world(ctx: &mut Context) {
    ctx.render_plain_text("Hello World");
}
fn hello_world2(ctx: &mut Context) {
    ctx.render_plain_text("Hello World2");
}
fn main() {
    let mut router = Router::new("/");
    router.get(hello_world);
    router.minion("hello2").get(hello_world2);
    let server = Server::new(router);
    hyper::rt::run(server.serve());
}