use salvo::prelude::*;

#[fn_handler]
async fn index(req: &mut Request, res: &mut Response) {
    res.render_plain_text(&format!("remote address: {:?}", req.remote_addr()));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(index);
    Server::bind(&"127.0.0.1:7878".parse().unwrap())
        .serve(Service::new(router))
        .await
        .unwrap();
}
