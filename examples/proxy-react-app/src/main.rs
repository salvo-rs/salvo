use salvo::extra::proxy::Proxy;
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**rest>").handle(Proxy::new(vec!["http://localhost:3000".into()]));
    println!("{:?}", router);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
