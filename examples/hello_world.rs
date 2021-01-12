use salvo::prelude::*;
use tracing;
use tracing_futures::Instrument;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {

async fn hello_world(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
async fn hello_world2(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World2");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "tracing=info,hello_world=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();
    let router = Router::new().get(fn_one_handler(hello_world));
    let router = router.push(Router::new().path("hello2").get(hello_world2));
    let server = Server::with_addr(router, "127.0.0.1:7878");
    server.serve().instrument(tracing::info_span!("Server::serve")).await?;
    Ok(())
}
