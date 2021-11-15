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
    let listener = TcpListener::bind(([0, 0, 0, 0], 7878)).rustls(async {
        RustlsConfig::new()
            .with_cert_path("examples/tls/cert.pem")
            .with_key_path("examples/tls/key.rsa")
    });
    Server::new(listener).serve(router).await;
}
