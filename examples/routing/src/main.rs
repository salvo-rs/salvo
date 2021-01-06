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
    let mut router = Router::new("/");
    router.get(hello_world);
    router.scope("hello2").get(hello_world2);

    let r = router
        .filter(path!("abc"))
        .filter(path!("yyyyy"))
        .before(bh)
        .push(router!().filter(path!("zzz")).handle(ddd))
        .push(router!().filter(path!("zzz")).handle(ddd));

    let server = Server::new(router);
    server.serve().instrument(tracing::info_span!("Server::serve")).await?;
    Ok(())
}
