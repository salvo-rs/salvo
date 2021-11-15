use hyper::server::conn::AddrIncoming;

use salvo::listener::rustls::RustlsConfig;
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(res: &mut Response) {
    res.render_plain_text("Hello World");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let mut incoming = AddrIncoming::bind(&([0, 0, 0, 0], 7878).into()).unwrap();
    incoming.set_nodelay(true);

    let listener = RustlsListener::with_rustls_config(
        RustlsConfig::new()
            .with_cert_path("examples/tls/cert.pem")
            .with_key_path("examples/tls/key.rsa"),
        incoming,
    )
    .unwrap();
    Server::new(listener).serve(router).await;
}
