//! native_tls module
use super::{Acceptor, Listener, Accepted};

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
    impl<C> Stream for NativeTlsListener<C>
    where
        C: Stream + Send + Unpin + 'static,
        C::Item: Into<Identity>,
    {
        type Item = Result<NativeTlsStream, IoError>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.poll_accept(cx)
        }
    }
    #[tokio::test]
    async fn test_native_tls_listener() {
        let mut listener = NativeTlsListener::with_config(
            NativeTlsConfig::new()
                .with_pkcs12(include_bytes!("../../certs/identity.p12").as_ref())
                .with_password("mypass"),
        )
        .bind("127.0.0.1:0");
        let addr = listener.local_addr();

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

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 10);
    }
}
