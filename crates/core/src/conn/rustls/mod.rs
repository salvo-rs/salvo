//! rustls module
use std::io::{self, Error as IoError, ErrorKind};

pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::{Certificate, RootCertStore};

pub(crate) mod config;
pub use config::{Keycert, RustlsConfig};

pub mod listener;
pub use listener::RustlsListener;

#[inline]
pub(crate) fn read_trust_anchor(mut trust_anchor: &[u8]) -> io::Result<RootCertStore> {
    let certs = rustls_pemfile::certs(&mut trust_anchor)?;
    let mut store = RootCertStore::empty();
    for cert in certs {
        store
            .add(&Certificate(cert))
            .map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_rustls::rustls::{ClientConfig, ServerName};
    use tokio_rustls::TlsConnector;

    use super::*;

    #[tokio::test]
    async fn test_rustls_listener() {
        let mut listener = RustlsListener::with_config(RustlsConfig::new(
            Keycert::new()
                .with_key_path("certs/key.pem")
                .with_cert_path("certs/cert.pem"),
        ))
        .bind("127.0.0.1:0");
        let addr = listener.local_addr();

        tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let trust_anchor = include_bytes!("../../certs/chain.pem");
            let client_config = ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(read_trust_anchor(trust_anchor.as_slice()).unwrap())
                .with_no_client_auth();
            let connector = TlsConnector::from(Arc::new(client_config));
            let mut tls_stream = connector
                .connect(ServerName::try_from("testserver.com").unwrap(), stream)
                .await
                .unwrap();
            tls_stream.write_i32(518).await.unwrap();
        });

        let ransport { mut stream, .. } = listener.accept().await.unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 518);
    }
}
