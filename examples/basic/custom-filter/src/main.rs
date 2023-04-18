use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // only allow access from http://localhost:5800/, http://127.0.0.1:5800/ will get not found page.
    let router = Router::new()
        .filter_fn(|req, _| {
            let host = req.header::<String>("HOST").unwrap_or_default();
            host == "localhost:5800"
        })
        .get(hello);

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
