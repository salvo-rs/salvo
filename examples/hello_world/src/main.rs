use salvo::prelude::*;
use tracing;
use tracing_futures::Instrument;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[fn_handler]
async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
#[fn_handler]
async fn hello_world2(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World2");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "tracing=info,hello_world=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();
    let mut router = Router::new();
    router.filter(method::get()).handle(hello_world);
    router.push(Router::new().filter("hello2").handle(hello_world2));
    let server = Server::new(router);
    server.serve().instrument(tracing::info_span!("Server::serve")).await?;
    Ok(())
}
