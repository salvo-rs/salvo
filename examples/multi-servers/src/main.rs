use salvo::prelude::*;

#[handler]
async fn hello1() -> &'static str {
    "Server1: Hello World"
}
#[handler]
async fn hello2() -> &'static str {
    "Server2: Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router1 = Router::new().get(hello1);
    let router2 = Router::new().get(hello2);

    tokio::try_join!(
        Server::new(TcpListener::bind("127.0.0.1:7878"))
            .await
            .try_serve(router1),
        Server::new(TcpListener::bind("127.0.0.1:7979"))
            .await
            .try_serve(router2),
    )
    .unwrap();
}
