use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) {
    res.render(Redirect::found("https://www.rust-lang.org/"))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    let router = Router::new().get(hello);
    Server::new(acceptor).serve(router).await;
}
