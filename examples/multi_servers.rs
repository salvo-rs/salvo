use salvo::prelude::*;

#[fn_handler]
async fn hello_world1() -> &'static str {
    "Server1: Hello World"
}
#[fn_handler]
async fn hello_world2() -> &'static str {
    "Server2: Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router1 = Router::new().get(hello_world1);
    let router2 = Router::new().get(hello_world2);

    tokio::join!(
        Server::new(router1).bind(([0, 0, 0, 0], 7878)),
        Server::new(router2).bind(([0, 0, 0, 0], 6868))
    );
}
