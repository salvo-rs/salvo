use salvo::listeners::openssl::{Keycert, OpensslConfig};
use salvo::prelude::*;
use tokio::time::Duration;

#[handler]
async fn hello_world(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let listener = OpensslListener::with_config_stream(async_stream::stream! {
        loop {
            yield load_config();
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    })
    .bind("127.0.0.1:7878");
    tracing::info!("Listening on https://127.0.0.1:7878");
    Server::new(listener).serve(router).await;
}

fn load_config() -> OpensslConfig {
    OpensslConfig::new(
        Keycert::new()
            .with_cert(include_bytes!("../certs/cert.pem").as_ref())
            .with_key(include_bytes!("../certs/key.pem").as_ref()),
    )
}
