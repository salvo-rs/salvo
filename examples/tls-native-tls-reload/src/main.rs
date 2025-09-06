use salvo::conn::native_tls::NativeTlsConfig;
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
    let acceptor = TcpListener::new("0.0.0.0:8698")
        .native_tls(async_stream::stream! {
            loop {
                yield load_config();
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
        .bind()
        .await;
    Server::new(acceptor).serve(router).await;
}

fn load_config() -> NativeTlsConfig {
    NativeTlsConfig::new()
        .pkcs12(include_bytes!("../certs/identity.p12").to_vec())
        .password("mypass")
}
