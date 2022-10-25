//! native_tls module
pub mod listener;
pub use listener::NativeTlsListener;

mod config;
pub use config::NativeTlsConfig;

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::conn::{Acceptor, Accepted, Listener};

    #[tokio::test]
    async fn test_native_tls_listener() {
        let mut listener = NativeTlsListener::bind(
            NativeTlsConfig::new()
                .with_pkcs12(include_bytes!("../../../certs/identity.p12").as_ref())
                .with_password("mypass"),
            "127.0.0.1:0",
        );
        let mut acceptor = listener.into_acceptor().await.unwrap();
        let addr = acceptor.local_addrs().remove(0);
        let addr = addr.into_std().unwrap();

        tokio::spawn(async move {
            let connector = tokio_native_tls::TlsConnector::from(
                tokio_native_tls::native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .unwrap(),
            );
            let stream = TcpStream::connect(addr).await.unwrap();
            let mut stream = connector.connect("127.0.0.1", stream).await.unwrap();
            stream.write_i32(10).await.unwrap();
        });

        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 10);
    }
}
