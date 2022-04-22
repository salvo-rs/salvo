use salvo::listener::rustls::RustlsConfig;
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let config = RustlsConfig::new()
        .with_cert_path("examples/certs/end.cert")
        .with_key_path("examples/certs/end.rsa");
    tracing::info!("Listening on https://0.0.0.0:7878");
    let listener = RustlsListener::with_rustls_config(config).bind("0.0.0.0:7878");
    Server::new(listener).serve(router).await;
}
