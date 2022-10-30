// This is to avoid an import loop:
// h3 tests depend on having private access to the crate.
// They must be part of the crate so as not to break privacy.
// They also depend on quinn_impl which depends on the crate.
// Having a dev-dependency on quinn_impl would work as far as cargo is
// concerned, but quic traits wouldn't match between the "h3" crate that
// comes before quinn_impl and the one that comes after and runs the tests
#[path = "../quinn_impl.rs"]
mod quinn_impl;

mod connection;
mod request;

use std::{
    convert::TryInto,
    net::{Ipv6Addr, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use futures_util::StreamExt;
use rustls::{Certificate, PrivateKey};

use super::quinn::{Incoming, NewConnection, TransportConfig};
use crate::quic;
use quinn_impl::Connection;

pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .with_test_writer()
        .try_init();
}

#[derive(Clone)]
pub struct Pair {
    port: u16,
    cert: Certificate,
    key: PrivateKey,
    config: Arc<TransportConfig>,
}

impl Default for Pair {
    fn default() -> Self {
        let (cert, key) = build_certs();
        Self {
            cert,
            key,
            port: 0,
            config: Arc::new(TransportConfig::default()),
        }
    }
}

impl Pair {
    pub fn with_timeout(&mut self, duration: Duration) {
        Arc::get_mut(&mut self.config)
            .unwrap()
            .max_idle_timeout(Some(duration.try_into().expect("idle timeout duration invalid")))
            .initial_rtt(Duration::from_millis(10));
    }

    pub fn server_inner(&mut self) -> (super::Endpoint, Incoming) {
        let mut crypto = rustls::ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![self.cert.clone()], self.key.clone())
            .unwrap();
        crypto.max_early_data_size = u32::MAX;
        crypto.alpn_protocols = vec![b"h3".to_vec()];

        let mut server_config = crate::quinn::ServerConfig::with_crypto(crypto.into());
        server_config.transport = self.config.clone();
        let (endpoint, incoming) = crate::Endpoint::server(server_config, "[::]:0".parse().unwrap()).unwrap();

        self.port = endpoint.local_addr().unwrap().port();

        (endpoint, incoming)
    }

    pub fn server(&mut self) -> Server {
        let (endpoint, incoming) = self.server_inner();
        Server { endpoint, incoming }
    }

    pub async fn client_inner(&self) -> NewConnection {
        let addr = (Ipv6Addr::LOCALHOST, self.port)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.add(&self.cert).unwrap();
        let mut crypto = rustls::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        crypto.enable_early_data = true;
        crypto.alpn_protocols = vec![b"h3".to_vec()];

        let client_config = crate::quinn::ClientConfig::new(Arc::new(crypto));

        let mut client_endpoint = crate::Endpoint::client("[::]:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);
        client_endpoint.connect(addr, "localhost").unwrap().await.unwrap()
    }

    pub async fn client(&self) -> quinn_impl::Connection {
        Connection::new(self.client_inner().await)
    }
}

pub struct Server {
    pub endpoint: super::Endpoint,
    pub incoming: Incoming,
}

impl Server {
    pub async fn next(&mut self) -> impl quic::Connection<Bytes> {
        Connection::new(self.incoming.next().await.unwrap().await.unwrap())
    }
}

pub fn build_certs() -> (Certificate, PrivateKey) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key = PrivateKey(cert.serialize_private_key_der());
    let cert = Certificate(cert.serialize_der().unwrap());
    (cert, key)
}
