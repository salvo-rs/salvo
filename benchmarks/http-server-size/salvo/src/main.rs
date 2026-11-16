use salvo::conn::Acceptor;
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
            let address = acceptor.holdings()[0]
                .local_addr
                .clone()
                .into_std()
                .unwrap();
            #[cfg(feature = "path-route")]
            let router = Router::with_path("hello").get(hello);
            #[cfg(not(feature = "path-route"))]
            let router = Router::new().get(hello);
            println!("READY http://{address}");
            Server::new(acceptor).serve(router).await;
        });
}
