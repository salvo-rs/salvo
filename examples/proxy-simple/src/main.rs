use salvo::extra::proxy::Proxy;
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("google/<**rest>").handle(Proxy::new(vec!["https://www.google.com".into()])))
        .push(Router::with_path("baidu/<**rest>").handle(Proxy::new(vec!["https://www.baidu.com".into()])));
    println!("{:?}", router);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
