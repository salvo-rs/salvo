use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

use salvo::prelude::*;
use salvo_extra::proxy::ProxyHandler;

#[fn_handler]
async fn hello_world1(res: &mut Response) {
    res.render_plain_text("Hello World1");
}
#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "proxy=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();
    let router = Router::new().push(
        Router::new()
            .path("google/<**rest>")
            .handle(ProxyHandler::new(vec!["https://www.google.com".into()])),
    );
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
