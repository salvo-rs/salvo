use std::net::SocketAddr;

use listenfd::ListenFd;
use salvo::prelude::*;
use salvo::conn::{tcp::TcpAcceptor, };

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() -> Result<(), salvo::Error>  {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);

    let mut listenfd = ListenFd::from_env();
    let (addr, listener) = if let Some(listener) = listenfd.take_tcp_listener(0)? {
        (
            listener.local_addr()?,
            tokio::net::TcpListener::from_std(listener.into()).unwrap(),
        )
    } else {
        let addr: SocketAddr = format!(
            "{}:{}",
            std::env::var("HOST").unwrap_or("127.0.0.1".into()),
            std::env::var("PORT").unwrap_or("8080".into())
        )
        .parse().unwrap();
        (addr, tokio::net::TcpListener::bind(addr).await.unwrap())
    };

    tracing::info!("Listening on {}", addr);
    let acceptor = TcpAcceptor::try_from(listener).unwrap();
    Server::new(acceptor).serve(router).await;
    Ok(())
}
