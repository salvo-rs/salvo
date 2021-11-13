use salvo::prelude::*;

#[fn_handler]
async fn hello_world(res: &mut Response) {
    res.render_plain_text("Hello World");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let listener = TlsListener::builder()
        .with_cert_path("examples/tls/cert.pem")
        .with_key_path("examples/tls/key.rsa")
        .bind(([0, 0, 0, 0], 7878));
    Server::new(listener).serve(router).await;
}
