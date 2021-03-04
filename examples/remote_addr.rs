use salvo::prelude::*;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[fn_handler]
async fn index(req: &mut Request, res: &mut Response) {
    res.render_plain_text(&format!("Your address: {:?}", req.remote_addr()));
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "remote_addr=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();
    let router = Router::new()
        .get(index);
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
