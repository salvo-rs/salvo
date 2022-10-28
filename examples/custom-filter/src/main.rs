use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // only allow access from http://localhost:7878/, http://127.0.0.1:7878/ will get not found page.
    let router = Router::new()
        .filter_fn(|req, _| {
            let host = req.header::<String>("HOST").unwrap_or_default();
            host == "localhost:7878"
        })
        .get(hello);

    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}
