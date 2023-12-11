//! OpensslListener and utils.
mod config;
pub use config::{Keycert, OpensslConfig, SslAcceptorBuilder};

mod listener;
pub use listener::{OpensslAcceptor, OpensslListener};

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use openssl::ssl::{SslConnector, SslMethod};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_openssl::SslStream;

    use super::*;
    use crate::conn::{Accepted, Acceptor, Listener, TcpListener};

    #[tokio::test]
    async fn test_openssl_listener() {
        let mut acceptor = TcpListener::new("127.0.0.1:0")
            .openssl(OpensslConfig::new(
                Keycert::new()
                    .key_from_path("certs/key.pem")
                    .unwrap()
                    .cert_from_path("certs/cert.pem")
                    .unwrap(),
            ))
            .bind()
            .await;
        let addr = acceptor.holdings()[0].local_addr.clone().into_std().unwrap();

        tokio::spawn(async move {
            let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
            connector.set_ca_file("certs/chain.pem").unwrap();

            let ssl = connector
                .build()
                .configure()
                .unwrap()
                .into_ssl("testserver.com")
                .unwrap();

            let stream = TcpStream::connect(addr).await.unwrap();
            let mut tls_stream = SslStream::new(ssl, stream).unwrap();
            Pin::new(&mut tls_stream).connect().await.unwrap();

            tls_stream.write_i32(518).await.unwrap();
        });

        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 518);
    }
}
