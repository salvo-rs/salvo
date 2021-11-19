use salvo::prelude::*;

#[fn_handler]
async fn index(req: &mut Request, res: &mut Response) {
    res.render_plain_text(&format!("remote address: {:?}", req.remote_addr()));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(index);
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
