use salvo::prelude::*;
use salvo_extra::cors::CorsHandler;

#[fn_handler]
async fn hello() -> &'static str {
    "hello"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let cors_handler = CorsHandler::builder()
        .with_allow_origin("https://salvo.rs")
        .with_allow_methods(vec!["GET", "POST", "DELETE"])
        .build();

    let router = Router::with_hoop(cors_handler).get(hello);
    Server::bind(&"127.0.0.1:7878".parse().unwrap())
        .serve(Service::new(router))
        .await
        .unwrap();
}
