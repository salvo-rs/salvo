use salvo::prelude::*;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let listener = TcpListener::bind(([0, 0, 0, 0], 7878))
        .unwrap()
        .join(TcpListener::bind(([0, 0, 0, 0], 7979)).unwrap());
    Server::builder(listener).serve(Service::new(router)).await.unwrap();
}
