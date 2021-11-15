use tokio::time::Duration;

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
    let listener = RustlsListener::with_config_stream(async_stream::stream! {
        loop {
            yield load_rustls_config();
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    })
    .bind(([0, 0, 0, 0], 7878));
    Server::new(listener).serve(router).await;
}

fn load_rustls_config() -> RustlsConfig {
    RustlsConfig::new()
        .with_cert_path("examples/tls/cert.pem")
        .with_key_path("examples/tls/key.rsa")
}
