use salvo::prelude::*;

#[fn_handler]
async fn index(req: &mut Request, res: &mut Response) {
    res.render_plain_text(&format!("remote address: {:?}", req.remote_addr()));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(index);
    Server::new(TcpListener::bind("0.0.0.0:7878")).serve(router).await;
}
