use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}
#[handler]
async fn hello_zh() -> Result<&'static str, ()> {
    Ok("你好，世界！")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    let router = Router::new().get(hello).push(Router::with_path("你好").get(hello_zh));
    println!("{:?}", router);
    Server::new(acceptor)
        .fuse_factory(salvo::fuse::simple::SimpleFactory::new())
        .serve(router)
        .await;
}
