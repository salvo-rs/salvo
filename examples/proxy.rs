use salvo::prelude::*;
use salvo_extra::proxy::ProxyHandler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(
            Router::new()
                .path("google/<**rest>")
                .handle(ProxyHandler::new(vec!["https://www.google.com".into()])),
        )
        .push(
            Router::new()
                .path("baidu/<**rest>")
                .handle(ProxyHandler::new(vec!["https://www.baidu.com".into()])),
        );
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
