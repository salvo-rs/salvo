//! ACME supports.
//!
//! Reference: <https://datatracker.ietf.org/doc/html/rfc8555>
//! Reference: <https://datatracker.ietf.org/doc/html/rfc8737>
//!
//! * HTTP-01
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::listener::{AcmeListener, TcpListener};
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn hello_world() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut router = Router::new().get(hello_world);
//!     let listener = AcmeListener::builder()
//!         // .directory("letsencrypt", salvo::listener::acme::LETS_ENCRYPT_STAGING)
//!         .cache_path("acme/letsencrypt")
//!         .add_domain("acme-http01.salvo.rs")
//!         .http01_challege(&mut router)
//!         .addr("0.0.0.0:443")
//!         .await;
//!     tracing::info!("Listening on https://0.0.0.0:443");
//!     let acceptor = listener.join(TcpListener::new("0.0.0.0:80")).bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
//!
//! * TLS ALPN-01
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::listener::AcmeListener;
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn hello_world() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().get(hello_world);
//!     let listener = AcmeListener::builder()
//!         // .directory("letsencrypt", salvo::listener::acme::LETS_ENCRYPT_STAGING)
//!         .cache_path("acme/letsencrypt")
//!         .add_domain("acme-tls-alpn01.salvo.rs")
//!         .bind("0.0.0.0:443")
//!         .await;
//!     tracing::info!("Listening on https://0.0.0.0:443");
//!     Server::new(listener).serve(router).await;
//! }
//! ```

pub mod cache;
mod client;
mod config;
mod issuer;
mod jose;
mod key_pair;
mod resolver;

use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};
use std::io::{self, Error as IoError};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use client::AcmeClient;
use futures_util::{ready, Future};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use parking_lot::RwLock;
use resolver::{ResolveServerCert, ACME_TLS_ALPN_NAME};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::sign::{any_ecdsa_type, CertifiedKey};
use tokio_rustls::rustls::PrivateKey;

use crate::addr::SocketAddr;
use crate::http::StatusError;
use crate::listener::{IntoAddrIncoming, Listener};
use crate::transport::Transport;
use crate::{async_trait, Depot, FlowCtrl, Handler, Request, Response, Router};
use cache::AcmeCache;
pub use config::{AcmeConfig, AcmeConfigBuilder};

/// Letsencrypt production directory url
pub const LETS_ENCRYPT_PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
/// Letsencrypt stagging directory url
pub const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

/// Well known acme challenge path
pub(crate) const WELL_KNOWN_PATH: &str = "/.well-known/acme-challenge";

/// HTTP-01 challenge
const CHALLENGE_TYPE_HTTP_01: &str = "http-01";

/// TLS-ALPN-01 challenge
const CHALLENGE_TYPE_TLS_ALPN_01: &str = "tls-alpn-01";

/// Challenge type
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChallengeType {
    /// HTTP-01 challenge
    ///
    /// Reference: <https://letsencrypt.org/docs/challenge-types/#http-01-challenge>
    Http01,
    /// TLS-ALPN-01
    ///
    /// Reference: <https://letsencrypt.org/docs/challenge-types/#tls-alpn-01>
    TlsAlpn01,
}
impl Display for ChallengeType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ChallengeType::Http01 => f.write_str(CHALLENGE_TYPE_HTTP_01),
            ChallengeType::TlsAlpn01 => f.write_str(CHALLENGE_TYPE_TLS_ALPN_01),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Directory {
    pub(crate) new_nonce: String,
    pub(crate) new_account: String,
    pub(crate) new_order: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Identifier {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Problem {
    pub(crate) detail: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Challenge {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) url: String,
    pub(crate) token: String,
}

/// Handler for `HTTP-01` challenge.
pub(crate) struct Http01Handler {
    pub(crate) keys: Arc<RwLock<HashMap<String, String>>>,
}

#[async_trait]
impl Handler for Http01Handler {
    #[inline]
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        if let Some(token) = req.params().get("token") {
            let keys = self.keys.read();
            if let Some(value) = keys.get(token) {
                res.render(value);
            } else {
                tracing::error!(token = %token, "keys not found for token");
                res.render(token);
            }
        } else {
            res.set_status_error(StatusError::not_found().with_summary("token is not provide"));
        }
    }
}

/// A wrapper around an underlying listener which implements the ACME.
pub struct AcmeListener {
    incoming: AddrIncoming,
    server_config: Arc<ServerConfig>,
}

impl AcmeListener {
    /// Create `AcmeListenerBuilder`
    pub fn builder() -> AcmeListenerBuilder {
        AcmeListenerBuilder::new()
    }
}
/// AcmeListenerBuilder
pub struct AcmeListenerBuilder {
    config_builder: AcmeConfigBuilder,
    check_duration: Duration,
}
impl AcmeListenerBuilder {
    #[inline]
    fn new() -> Self {
        let config_builder = AcmeConfig::builder();
        Self {
            config_builder,
            check_duration: Duration::from_secs(10 * 60),
        }
    }

    /// Sets the directory.
    ///
    /// Defaults to lets encrypt.
    #[inline]
    pub fn get_directory(self, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.directory(name, url),
            ..self
        }
    }

    /// Sets domains.
    #[inline]
    pub fn domains(self, domains: impl Into<HashSet<String>>) -> Self {
        Self {
            config_builder: self.config_builder.domains(domains),
            ..self
        }
    }
    /// Add a domain.
    #[inline]
    pub fn add_domain(self, domain: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.add_domain(domain),
            ..self
        }
    }

    /// Add contact emails for the ACME account.
    #[inline]
    pub fn contacts(self, contacts: impl Into<HashSet<String>>) -> Self {
        Self {
            config_builder: self.config_builder.contacts(contacts.into()),
            ..self
        }
    }
    /// Add a contact email for the ACME account.
    #[inline]
    pub fn add_contact(self, contact: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.add_contact(contact.into()),
            ..self
        }
    }

    /// Create an handler for HTTP-01 challenge
    #[inline]
    pub fn http01_challege(self, router: &mut Router) -> Self {
        let config_builder = self.config_builder.http01_challege();
        if let Some(keys_for_http01) = &config_builder.keys_for_http01 {
            let handler = Http01Handler {
                keys: keys_for_http01.clone(),
            };
            router
                .routers
                .push(Router::with_path(format!("{}/<token>", WELL_KNOWN_PATH)).handle(handler));
        } else {
            panic!("`HTTP-01` challage's key should not none");
        }
        Self { config_builder, ..self }
    }
    /// Create an handler for HTTP-01 challenge
    #[inline]
    pub fn tls_alpn01_challege(self) -> Self {
        Self {
            config_builder: self.config_builder.tls_alpn01_challege(),
            ..self
        }
    }

    /// Sets the cache path for caching certificates.
    ///
    /// This is not a necessary option. If you do not configure the cache path,
    /// the obtained certificate will be stored in memory and will need to be
    /// obtained again when the server is restarted next time.
    #[inline]
    pub fn cache_path(self, path: impl Into<PathBuf>) -> Self {
        Self {
            config_builder: self.config_builder.cache_path(path),
            ..self
        }
    }

    /// Consumes this builder and returns a [`AcmeListener`] object.
    #[inline]
    pub async fn bind(self, incoming: impl IntoAddrIncoming) -> AcmeListener {
        self.try_bind(incoming).await.unwrap()
    }
    /// Consumes this builder and returns a [`Result<AcmeListener, std::IoError>`] object.
    pub async fn try_bind(self, incoming: impl IntoAddrIncoming) -> Result<AcmeListener, crate::Error> {
        let Self {
            config_builder,
            check_duration,
        } = self;
        let acme_config = config_builder.build()?;

        let mut client = AcmeClient::try_new(
            &acme_config.directory_url,
            acme_config.key_pair.clone(),
            acme_config.contacts.clone(),
        )
        .await?;

        let mut cached_key = None;
        let mut cached_cert = None;
        if let Some(cache_path) = &acme_config.cache_path {
            let key_data = cache_path
                .read_key(&acme_config.directory_name, &acme_config.domains)
                .await?;
            if let Some(key_data) = key_data {
                tracing::debug!("load private key from cache");
                match rustls_pemfile::pkcs8_private_keys(&mut key_data.as_slice()) {
                    Ok(key) => cached_key = key.into_iter().next(),
                    Err(err) => {
                        tracing::warn!("failed to parse cached private key: {}", err)
                    }
                };
            }
            let cert_data = cache_path
                .read_cert(&acme_config.directory_name, &acme_config.domains)
                .await?;
            if let Some(cert_data) = cert_data {
                tracing::debug!("load certificate from cache");
                match rustls_pemfile::certs(&mut cert_data.as_slice()) {
                    Ok(cert) => cached_cert = Some(cert),
                    Err(err) => {
                        tracing::warn!("failed to parse cached tls certificates: {}", err)
                    }
                };
            }
        };

        let cert_resolver = Arc::new(ResolveServerCert::default());
        if let (Some(cached_cert), Some(cached_key)) = (cached_cert, cached_key) {
            let certs = cached_cert
                .into_iter()
                .map(tokio_rustls::rustls::Certificate)
                .collect::<Vec<_>>();
            tracing::debug!("using cached tls certificates");
            *cert_resolver.cert.write() = Some(Arc::new(CertifiedKey::new(
                certs,
                any_ecdsa_type(&PrivateKey(cached_key)).unwrap(),
            )));
        }

        let weak_cert_resolver = Arc::downgrade(&cert_resolver);
        let mut server_config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_cert_resolver(cert_resolver);

        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        if acme_config.challenge_type == ChallengeType::TlsAlpn01 {
            server_config.alpn_protocols.push(ACME_TLS_ALPN_NAME.to_vec());
        }

        let listener = AcmeListener {
            incoming: incoming.into_incoming()?,
            server_config: Arc::new(server_config),
        };

        tokio::spawn(async move {
            while let Some(cert_resolver) = Weak::upgrade(&weak_cert_resolver) {
                if cert_resolver.will_expired(acme_config.before_expired) {
                    if let Err(err) = issuer::issue_cert(&mut client, &acme_config, &cert_resolver).await {
                        tracing::error!(error = %err, "failed to issue certificate");
                    }
                }
                tokio::time::sleep(check_duration).await;
            }
        });

        Ok(listener)
    }
}

impl Listener for AcmeListener {}

#[async_trait::async_trait]
impl Accept for AcmeListener {
    type Conn = AcmeStream;
    type Error = IoError;

    #[inline]
    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let this = self.get_mut();
        match ready!(Pin::new(&mut this.incoming).poll_accept(cx)) {
            Some(Ok(sock)) => Poll::Ready(Some(Ok(AcmeStream::new(sock, this.server_config.clone())))),
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }
}

enum AcmeState {
    Handshaking(tokio_rustls::Accept<AddrStream>),
    Streaming(tokio_rustls::server::TlsStream<AddrStream>),
}

/// tokio_rustls::server::TlsStream doesn't expose constructor methods,
/// so we have to TlsAcceptor::accept and handshake to have access to it
/// AcmeStream implements AsyncRead/AsyncWrite handshaking tokio_rustls::Accept first
pub struct AcmeStream {
    state: AcmeState,
    remote_addr: SocketAddr,
}
impl Transport for AcmeStream {
    #[inline]
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr.clone())
    }
}

impl AcmeStream {
    #[inline]
    fn new(stream: AddrStream, config: Arc<ServerConfig>) -> Self {
        let remote_addr = stream.remote_addr();
        let accept = tokio_rustls::TlsAcceptor::from(config).accept(stream);
        AcmeStream {
            state: AcmeState::Handshaking(accept),
            remote_addr: remote_addr.into(),
        }
    }
}

impl AsyncRead for AcmeStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            AcmeState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = AcmeState::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            AcmeState::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for AcmeStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            AcmeState::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = AcmeState::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            AcmeState::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            AcmeState::Handshaking(_) => Poll::Ready(Ok(())),
            AcmeState::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            AcmeState::Handshaking(_) => Poll::Ready(Ok(())),
            AcmeState::Streaming(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}
