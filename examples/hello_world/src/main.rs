use hyper;
use novel::prelude::*;

fn hello_world(_sconf: Arc<ServerConfig>, _req: &Request, _depot: &mut Depot, resp: &mut Response) {
    resp.render_plain_text("Hello World");
}
fn hello_world2(_sconf: Arc<ServerConfig>, _req: &Request, _depot: &mut Depot, resp: &mut Response) {
    resp.render_plain_text("Hello World2");
}
fn main() {
    let mut router = Router::new("/");
    router.get(hello_world);
    router.minion("hello2").get(hello_world2);
    let server = Server::new(router);
    hyper::rt::run(server.serve());
}