use salvo::prelude::*;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[fn_handler]
async fn hello_world1(res: &mut Response) {
    res.render_plain_text("Hello World1");
}
#[fn_handler]
async fn hello_world2(_req: &mut Request, res: &mut Response) {
    res.render_plain_text("Hello World2");
}
#[fn_handler]
async fn hello_world3(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World3");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "hello_world=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();
    let router = Router::new()
        .get(hello_world1)
        .push(Router::new().path("hello2").get(hello_world2))
        .push(Router::new().path("hello3").get(hello_world3));
    Server::new(router).bind(([127, 0, 0, 1], 7878)).await;
    Ok(())
}
