#![allow(dead_code)]

use std::io;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::rt::{AsyncRead, AsyncWrite, TlsStream};

use crate::Error;
use rbs::err_protocol;
use std::mem::replace;

/// X.509 Certificate input, either a file path or a PEM encoded inline certificate(s).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum CertificateInput {
    /// PEM encoded certificate(s)
    Inline(Vec<u8>),
    /// Path to a file containing PEM encoded certificate(s)
    File(PathBuf),
}

impl From<String> for CertificateInput {
    fn from(value: String) -> Self {
        let trimmed = value.trim();
        // Some heuristics according to https://tools.ietf.org/html/rfc7468
        if trimmed.starts_with("-----BEGIN CERTIFICATE-----")
            && trimmed.contains("-----END CERTIFICATE-----")
        {
            CertificateInput::Inline(value.as_bytes().to_vec())
        } else {
            CertificateInput::File(PathBuf::from(value))
        }
    }
}

impl CertificateInput {
    async fn data(&self) -> Result<Vec<u8>, std::io::Error> {
        use crate::rt::fs;
        match self {
            CertificateInput::Inline(v) => Ok(v.clone()),
            CertificateInput::File(path) => fs::read(path).await,
        }
    }
}

impl std::fmt::Display for CertificateInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CertificateInput::Inline(v) => write!(f, "{}", String::from_utf8_lossy(v.as_slice())),
            CertificateInput::File(path) => write!(f, "file: {}", path.display()),
        }
    }
}

#[cfg(feature = "tls-rustls")]
mod rustls;

pub enum MaybeTlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    Raw(S),
    Tls(TlsStream<S>),
    Upgrading,
}

impl<S> MaybeTlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    #[inline]
    pub fn is_tls(&self) -> bool {
        matches!(self, Self::Tls(_))
    }

    pub async fn upgrade(
        &mut self,
        host: &str,
        accept_invalid_certs: bool,
        accept_invalid_hostnames: bool,
        root_cert_path: Option<&CertificateInput>,
    ) -> Result<(), Error> {
        // Already upgraded: leave the existing TLS stream untouched.
        if matches!(self, MaybeTlsStream::Tls(_)) {
            return Ok(());
        }

        let connector = configure_tls_connector(
            accept_invalid_certs,
            accept_invalid_hostnames,
            root_cert_path,
        )
        .await?;

        let stream = match replace(self, MaybeTlsStream::Upgrading) {
            MaybeTlsStream::Raw(stream) => stream,

            MaybeTlsStream::Tls(_) => {
                // ignore upgrade, we are already a TLS connection
                return Ok(());
            }

            MaybeTlsStream::Upgrading => {
                // we previously failed to upgrade and now hold no connection
                // this should only happen from an internal misuse of this method
                return Err(Error::from(io::ErrorKind::ConnectionAborted.to_string()));
            }
        };

        #[cfg(feature = "tls-rustls")]
        let host = tokio_rustls::rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|err| Error::from(err.to_string()))?;

        *self = MaybeTlsStream::Tls(
            connector
                .connect(host, stream)
                .await
                .map_err(|err| err_protocol!("{}", err))?,
        );

        Ok(())
    }
}

#[cfg(feature = "tls-native-tls")]
async fn configure_tls_connector(
    accept_invalid_certs: bool,
    accept_invalid_hostnames: bool,
    root_cert_path: Option<&CertificateInput>,
) -> Result<crate::rt::TlsConnector, Error> {
    use crate::rt::native_tls::{Certificate, TlsConnector};

    let mut builder = TlsConnector::builder();
    builder
        .danger_accept_invalid_certs(accept_invalid_certs)
        .danger_accept_invalid_hostnames(accept_invalid_hostnames);

    if !accept_invalid_certs {
        if let Some(ca) = root_cert_path {
            let data = ca.data().await.map_err(|err| err_protocol!("{}", err))?;
            let cert = Certificate::from_pem(&data).map_err(|err| err_protocol!("{}", err))?;

            builder.add_root_certificate(cert);
        }
    }
    let connector = builder
        .build()
        .map_err(|err| err_protocol!("{}", err))?
        .into();

    Ok(connector)
}

#[cfg(feature = "tls-rustls")]
use self::rustls::configure_tls_connector;

impl<S> AsyncRead for MaybeTlsStream<S>
where
    S: Unpin + AsyncWrite + AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut super::PollReadBuf<'_>,
    ) -> Poll<io::Result<super::PollReadOut>> {
        match &mut *self {
            MaybeTlsStream::Raw(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_read(cx, buf),

            MaybeTlsStream::Upgrading => Poll::Ready(Err(io::ErrorKind::ConnectionAborted.into())),
        }
    }
}

impl<S> AsyncWrite for MaybeTlsStream<S>
where
    S: Unpin + AsyncWrite + AsyncRead,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut *self {
            MaybeTlsStream::Raw(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_write(cx, buf),

            MaybeTlsStream::Upgrading => Poll::Ready(Err(io::ErrorKind::ConnectionAborted.into())),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            MaybeTlsStream::Raw(s) => Pin::new(s).poll_flush(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_flush(cx),

            MaybeTlsStream::Upgrading => Poll::Ready(Err(io::ErrorKind::ConnectionAborted.into())),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            MaybeTlsStream::Raw(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_shutdown(cx),

            MaybeTlsStream::Upgrading => Poll::Ready(Err(io::ErrorKind::ConnectionAborted.into())),
        }
    }
}

impl<S> Deref for MaybeTlsStream<S>
where
    S: Unpin + AsyncWrite + AsyncRead,
{
    type Target = S;

    fn deref(&self) -> &Self::Target {
        match self {
            MaybeTlsStream::Raw(s) => s,

            #[cfg(feature = "tls-rustls")]
            MaybeTlsStream::Tls(s) => s.get_ref().0,

            #[cfg(feature = "tls-native-tls")]
            MaybeTlsStream::Tls(s) => s.get_ref().get_ref().get_ref(),

            MaybeTlsStream::Upgrading => {
                panic!(
                    "TLS stream accessed while connection is in upgrading state - this indicates a race condition or incorrect state management"
                );
            }
        }
    }
}

impl<S> DerefMut for MaybeTlsStream<S>
where
    S: Unpin + AsyncWrite + AsyncRead,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            MaybeTlsStream::Raw(s) => s,

            #[cfg(feature = "tls-rustls")]
            MaybeTlsStream::Tls(s) => s.get_mut().0,

            #[cfg(feature = "tls-native-tls")]
            MaybeTlsStream::Tls(s) => s.get_mut().get_mut().get_mut(),

            MaybeTlsStream::Upgrading => {
                panic!(
                    "TLS stream accessed while connection is in upgrading state - this indicates a race condition or incorrect state management"
                );
            }
        }
    }
}
