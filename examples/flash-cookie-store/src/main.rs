use std::fmt::Write;

use salvo::flash::{CookieStore, FlashDepotExt};
use salvo::prelude::*;

#[handler]
pub async fn set_flash(depot: &mut Depot, res: &mut Response) {
    let flash = depot.outgoing_flash_mut();
    flash.info("Hey there!").debug("How is it going?");
    res.render(Redirect::other("/get"));
}

#[handler]
pub async fn get_flash(depot: &mut Depot, _res: &mut Response) -> String {
    let mut body = String::new();
    if let Some(flash) = depot.incoming_flash() {
        for message in flash.iter() {
            writeln!(body, "{} - {}", message.value, message.level).unwrap();
        }
    }
    body
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .hoop(CookieStore::new().into_handler())
        .push(Router::with_path("get").get(get_flash))
        .push(Router::with_path("set").get(set_flash));
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
