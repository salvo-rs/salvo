use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;
use tokio::time::Duration;

#[handler]
async fn hello(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let listener = RustlsListener::bind(
        async_stream::stream! {
            loop {
                yield load_config();
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        },
        "127.0.0.1:7878",
    );
    Server::new(listener).serve(router).await;
}

fn load_config() -> RustlsConfig {
    RustlsConfig::new(
        Keycert::new()
            .with_cert(include_bytes!("../certs/cert.pem").as_ref())
            .with_key(include_bytes!("../certs/key.pem").as_ref()),
    )
}
