use salvo::conn::native_tls::NativeTlsConfig;
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
    let listener = NativeTlsListener::bind(
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

fn load_config() -> NativeTlsConfig {
    NativeTlsConfig::new()
        .with_pkcs12(include_bytes!("../certs/identity.p12").to_vec())
        .with_password("mypass")
}
