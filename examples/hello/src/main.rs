use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}
#[handler]
async fn hello_zh() -> Result<&'static str, ()> {
    Ok("你好，世界！")
}
#[handler]
async fn hello2(res: &mut Response) {
    res.render("Hello World2");
}
#[handler]
async fn hello3(_req: &mut Request, res: &mut Response) {
    res.render(Text::Plain("Hello World3"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    let router = route();
    println!("{:?}", router);
    Server::new(acceptor).serve(router).await;
}

fn route() -> Router {
    Router::new().get(hello)
    .push(Router::with_path("a").get(hello2))
        .push(Router::with_path("你好").get(hello_zh))
        .push(Router::with_path("hello3").get(hello3))
}
