use axum::{Router, routing::get};

async fn hello() -> &'static str {
    "Hello World"
}

fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            #[cfg(feature = "path-route")]
            let app = Router::new().route("/hello", get(hello));
            #[cfg(not(feature = "path-route"))]
            let app = Router::new().route("/", get(hello));
            println!("READY http://{address}");
            axum::serve(listener, app).await.unwrap();
        });
}
