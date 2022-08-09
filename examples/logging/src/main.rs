use salvo::extra::logging::LogHandler;
use salvo::prelude::*;

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}
#[handler]
async fn hello_world1() -> Result<&'static str, ()> {
    Ok("Hello World1")
}
#[handler]
async fn hello_world2(res: &mut Response) {
    res.render(Text::Plain("Hello World2"));
}
#[handler]
async fn hello_world3(_req: &mut Request, res: &mut Response) {
    res.render(Text::Plain("Hello World3"));
}
#[handler]
async fn hello_world4(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render(Text::Plain("Hello World4"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .hoop(LogHandler)
        .get(hello_world)
        .push(Router::with_path("hello1").get(hello_world1))
        .push(Router::with_path("hello2").get(hello_world2))
        .push(Router::with_path("hello3").get(hello_world3))
        .push(Router::with_path("hello4").get(hello_world4));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
