//! `RustlsListener` and utils.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};

use tokio_rustls::rustls::RootCertStore;

pub(crate) mod config;
pub use config::{Keycert, RustlsConfig, ServerConfig};

mod listener;
pub use listener::{RustlsAcceptor, RustlsListener};

pub(crate) fn read_trust_anchor(mut trust_anchor: &[u8]) -> IoResult<RootCertStore> {
    let certs = rustls_pemfile::certs(&mut trust_anchor).collect::<IoResult<Vec<_>>>()?;
    let mut store = RootCertStore::empty();
    for cert in certs {
        store
            .add(cert)
            .map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_rustls::rustls::{pki_types::ServerName, ClientConfig};
    use tokio_rustls::TlsConnector;

    use super::*;
    use crate::conn::{Accepted, Acceptor, Listener, TcpListener};

    #[tokio::test]
    async fn test_rustls_listener() {
        let mut acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(RustlsConfig::new(
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
            let stream = TcpStream::connect(addr).await.unwrap();
            let trust_anchor = include_bytes!("../../../certs/chain.pem");
            let client_config = ClientConfig::builder()
                .with_root_certificates(read_trust_anchor(trust_anchor.as_slice()).unwrap())
                .with_no_client_auth();
            let connector = TlsConnector::from(Arc::new(client_config));
            let mut tls_stream = connector
                .connect(ServerName::try_from("testserver.com").unwrap(), stream)
                .await
                .unwrap();
            tls_stream.write_i32(518).await.unwrap();
        });

        let Accepted { mut conn, .. } = acceptor.accept(None).await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 518);
    }
}
