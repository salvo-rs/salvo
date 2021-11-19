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
    let config = RustlsConfig::new()
        .with_cert_path("examples/tls/cert.pem")
        .with_key_path("examples/tls/key.rsa");
        tracing::info!("Listening on http://127.0.0.1:7878");
    let listener = RustlsListener::with_rustls_config(config).bind("127.0.0.1:7878");
    Server::new(listener).serve(router).await;
}
