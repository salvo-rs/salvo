/// TLS metadata shared by all requests on a connection.
///
/// Retrieved with [`Request::tls_info`](crate::Request::tls_info). Certificate
/// bytes are captured once after the handshake; reading them does not copy or
/// parse certificates. Salvo's Rustls listener and Rustls-backed Quinn listener
/// populate this metadata. Other TLS backends currently do not.
#[derive(Debug)]
pub struct TlsInfo {
    pub(crate) peer_certificates: Vec<Vec<u8>>,
    pub(crate) server_name: Option<String>,
    pub(crate) alpn_protocol: Option<Vec<u8>>,
}

impl TlsInfo {
    /// Returns the peer's DER-encoded certificate chain, leaf first.
    ///
    /// On the server these are the client's certificates. An empty slice means
    /// the TLS connection has no client certificate, for example when client
    /// authentication is disabled or an optional-authentication client is anonymous.
    /// Verification follows the listener's TLS configuration; applications using
    /// a custom verifier must enforce their own trust policy. With a custom
    /// Rustls raw-public-key verifier, the single entry is a DER SubjectPublicKeyInfo
    /// rather than an X.509 certificate, matching Rustls's `peer_certificates` API.
    #[must_use]
    pub fn peer_certificates(&self) -> &[Vec<u8>] {
        &self.peer_certificates
    }

    /// Returns the server name requested through SNI, when provided by the client.
    #[must_use]
    pub fn server_name(&self) -> Option<&str> {
        self.server_name.as_deref()
    }

    /// Returns the negotiated ALPN protocol, for example `b"h2"`.
    #[must_use]
    pub fn alpn_protocol(&self) -> Option<&[u8]> {
        self.alpn_protocol.as_deref()
    }

    #[cfg(feature = "rustls")]
    pub(crate) fn from_rustls(connection: &rustls::ServerConnection) -> Self {
        Self {
            peer_certificates: connection
                .peer_certificates()
                .unwrap_or_default()
                .iter()
                .map(|cert| cert.as_ref().to_vec())
                .collect(),
            server_name: connection.server_name().map(str::to_owned),
            alpn_protocol: connection.alpn_protocol().map(<[u8]>::to_vec),
        }
    }

    #[cfg(feature = "quinn")]
    pub(crate) fn from_quinn(connection: &quinn::Connection) -> Option<Self> {
        let handshake = connection
            .handshake_data()?
            .downcast::<quinn::crypto::rustls::HandshakeData>()
            .ok()?;
        let certificates = connection
            .peer_identity()
            .and_then(|identity| {
                identity
                    .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                    .ok()
            })
            .map(|chain| chain.iter().map(|cert| cert.as_ref().to_vec()).collect())
            .unwrap_or_default();
        Some(Self {
            peer_certificates: certificates,
            server_name: handshake.server_name,
            alpn_protocol: handshake.protocol,
        })
    }
}
