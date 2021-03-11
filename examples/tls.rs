use salvo::prelude::*;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[fn_handler]
async fn hello_world(res: &mut Response) {
    res.render_plain_text("Hello World");
}

// Don't copy this `cfg`, it's only needed because this file is within
// the salvo repository.
#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "hello_world=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();
    let router = Router::new().get(hello_world);
    Server::new(router)
        .tls()
        .cert_path("examples/tls/cert.pem")
        .key_path("examples/tls/key.rsa")
        .bind(([0, 0, 0, 0], 3030))
        .await;
}