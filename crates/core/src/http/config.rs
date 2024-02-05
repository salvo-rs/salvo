

#[derive(Copy, Clone)]
pub struct HttpConfig {
    connect_idle_timeout: Duration,
    tls_handshake_timeout: Duration,
    disconnect_timeout: Duration,
    headers_timeout: Duration,
    body_chunk_timeout: Duration,

    body_secure_max_size: u64,
}