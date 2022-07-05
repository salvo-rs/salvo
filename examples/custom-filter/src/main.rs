use salvo::prelude::*;

#[fn_handler]
async fn hello_world() -> &'static str {
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
        .get(hello_world);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
