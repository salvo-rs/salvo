use salvo::conn::native_tls::NativeTlsConfig;
use salvo::prelude::*;

use tracing::Level;

#[handler]
async fn hello(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();

    let router = Router::new().get(hello);
    let config = NativeTlsConfig::new()
        .pkcs12(include_bytes!("../certs/identity.p12").to_vec())
        .password("mypass");
    let acceptor = TcpListener::new("127.0.0.1:5800").native_tls(config).bind().await;
    Server::new(acceptor).serve(router).await;
}
