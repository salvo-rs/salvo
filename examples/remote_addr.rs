use salvo::prelude::*;

#[fn_handler]
async fn index(req: &mut Request, res: &mut Response) {
    res.render(Text::Plain(format!("remote address: {:?}", req.remote_addr())));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(index);
    tracing::info!("Listening on http://0.0.0.0:7878");
    Server::new(TcpListener::bind("0.0.0.0:7878")).serve(router).await;
}
