//! Tests for `rbdc::net` (TCP/Unix sockets and the TLS stream wrapper).

use rbdc::net::{CertificateInput, MaybeTlsStream, Socket};
use std::path::PathBuf;
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Socket
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_tcp_socket_echo() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = tokio::spawn(async move {
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4];
        peer.read_exact(&mut buf).await.unwrap();
        peer.write_all(b"pong").await.unwrap();
        peer.flush().await.unwrap();
        peer.shutdown().await.unwrap();
    });

    let mut socket = Socket::connect_tcp("127.0.0.1", port).await.unwrap();
    socket.write_all(b"ping").await.unwrap();
    socket.flush().await.unwrap();

    let mut buf = [0u8; 4];
    socket.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pong");
    // Exercise AsyncWrite::poll_shutdown for the TCP variant.
    tokio::io::AsyncWriteExt::shutdown(&mut socket).await.unwrap();

    server.await.unwrap();
}

#[tokio::test]
async fn test_tcp_socket_ipv6_bracket_trimming() {
    // IPv6 loopback may be unavailable in some sandboxes; skip if so.
    let Ok(listener) = tokio::net::TcpListener::bind("[::1]:0").await else {
        return;
    };
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut peer, _) = listener.accept().await.unwrap();
        peer.write_all(b"v6").await.unwrap();
        peer.shutdown().await.unwrap();
    });

    let mut socket = Socket::connect_tcp("[::1]", port).await.unwrap();
    let mut buf = [0u8; 2];
    socket.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"v6");
    socket.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn test_tcp_socket_connection_refused() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert!(Socket::connect_tcp("127.0.0.1", port).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn test_unix_socket_echo() {
    use tokio::net::UnixListener;

    let path = std::env::temp_dir().join(format!(
        "rbdc_socket_test_{}_{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let path_for_server = path.clone();

    let server = tokio::spawn(async move {
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 2];
        peer.read_exact(&mut buf).await.unwrap();
        peer.write_all(b"ok").await.unwrap();
        peer.shutdown().await.unwrap();
        let _ = std::fs::remove_file(&path_for_server);
    });

    let mut socket = Socket::connect_uds(&path).await.unwrap();
    socket.write_all(b"hi").await.unwrap();
    socket.flush().await.unwrap();
    let mut buf = [0u8; 2];
    socket.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ok");
    // The peer already shut down its write side; a second shutdown may
    // legitimately report ENOTCONN on some platforms. This still exercises
    // both the inherent `Socket::shutdown` and `AsyncWrite::poll_shutdown`
    // implementations for the Unix variant.
    let _ = socket.shutdown().await;
    let _ = tokio::io::AsyncWriteExt::shutdown(&mut socket).await;
    server.await.unwrap();
}

#[cfg(not(unix))]
#[tokio::test]
async fn test_unix_socket_not_supported_on_windows() {
    let err = Socket::connect_uds("does-not-exist").await.unwrap_err();
    assert!(err.to_string().to_lowercase().contains("unix"));
}

// ---------------------------------------------------------------------------
// CertificateInput
// ---------------------------------------------------------------------------

#[test]
fn test_certificate_input_from_string() {
    let pem = "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----";
    let inline: CertificateInput = pem.to_string().into();
    assert!(matches!(inline, CertificateInput::Inline(_)));
    assert!(inline.to_string().contains("BEGIN CERTIFICATE"));
    assert_eq!(format!("{inline:?}").contains("Inline"), true);

    let file: CertificateInput = "certs/root.pem".to_string().into();
    assert!(matches!(file, CertificateInput::File(_)));
    assert_eq!(file.to_string(), "file: certs/root.pem");
    assert!(matches!(
        file,
        CertificateInput::File(ref p) if p == &PathBuf::from("certs/root.pem")
    ));

    // Only a complete BEGIN/END pair counts as inline PEM.
    let half: CertificateInput = "-----BEGIN CERTIFICATE-----".to_string().into();
    assert!(matches!(half, CertificateInput::File(_)));

    let json = serde_json::to_string(&file).unwrap();
    let back: CertificateInput = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, CertificateInput::File(_)));
}

// ---------------------------------------------------------------------------
// MaybeTlsStream
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_maybe_tls_raw_read_write() {
    let (client, mut server) = tokio::io::duplex(64);
    let mut stream: MaybeTlsStream<tokio::io::DuplexStream> = MaybeTlsStream::Raw(client);
    assert!(!stream.is_tls());

    stream.write_all(b"hi").await.unwrap();
    stream.flush().await.unwrap();
    let mut buf = [0u8; 2];
    server.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hi");

    server.write_all(b"yo").await.unwrap();
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"yo");

    // Deref / DerefMut to the underlying stream.
    let _ = &*stream;
    let _ = &mut *stream;

    stream.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_maybe_tls_upgrading_poll_errors() {
    use tokio::io::{AsyncRead, AsyncWrite};

    let mut stream: MaybeTlsStream<tokio::io::DuplexStream> = MaybeTlsStream::Upgrading;
    let mut buf = [0u8; 1];

    let read = futures_util::future::poll_fn(|cx| {
        let mut read_buf = rbdc::rt::ReadBuf::new(&mut buf);
        Pin::new(&mut stream).poll_read(cx, &mut read_buf)
    })
    .await;
    assert_eq!(
        read.unwrap_err().kind(),
        std::io::ErrorKind::ConnectionAborted
    );

    let written = futures_util::future::poll_fn(|cx| {
        Pin::new(&mut stream).poll_write(cx, b"x")
    })
    .await;
    assert_eq!(
        written.unwrap_err().kind(),
        std::io::ErrorKind::ConnectionAborted
    );

    let flushed = futures_util::future::poll_fn(|cx| Pin::new(&mut stream).poll_flush(cx)).await;
    assert_eq!(
        flushed.unwrap_err().kind(),
        std::io::ErrorKind::ConnectionAborted
    );

    let shutdown =
        futures_util::future::poll_fn(|cx| Pin::new(&mut stream).poll_shutdown(cx)).await;
    assert_eq!(
        shutdown.unwrap_err().kind(),
        std::io::ErrorKind::ConnectionAborted
    );
}

#[test]
#[should_panic(expected = "TLS stream accessed while connection is in upgrading state")]
fn test_maybe_tls_upgrading_deref_panics() {
    let stream: MaybeTlsStream<tokio::io::DuplexStream> = MaybeTlsStream::Upgrading;
    let _ = &*stream;
}

#[test]
#[should_panic(expected = "TLS stream accessed while connection is in upgrading state")]
fn test_maybe_tls_upgrading_deref_mut_panics() {
    let mut stream: MaybeTlsStream<tokio::io::DuplexStream> = MaybeTlsStream::Upgrading;
    let _ = &mut *stream;
}

#[tokio::test]
async fn test_upgrade_with_missing_cert_file_errors() {
    let (client, _server) = tokio::io::duplex(64);
    let mut stream = MaybeTlsStream::Raw(client);
    let ca = CertificateInput::File(PathBuf::from("/definitely/not/a/real/cert.pem"));
    let err = stream
        .upgrade("example.com", false, false, Some(&ca))
        .await
        .unwrap_err();
    assert!(!err.to_string().is_empty());
    assert!(!stream.is_tls());
}

#[tokio::test]
async fn test_upgrade_with_invalid_inline_cert_errors() {
    let (client, _server) = tokio::io::duplex(64);
    let mut stream = MaybeTlsStream::Raw(client);
    let ca: CertificateInput =
        "-----BEGIN CERTIFICATE-----\nnot-valid-base64\n-----END CERTIFICATE-----"
            .to_string()
            .into();
    let err = stream
        .upgrade("example.com", false, false, Some(&ca))
        .await
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn test_upgrade_with_invalid_hostname_verifier_handshake_fails() {
    // Exercises the accept_invalid_hostnames verifier construction path.
    let (client, server) = tokio::io::duplex(64);
    // Close the peer so the handshake fails immediately instead of waiting.
    drop(server);
    let mut stream = MaybeTlsStream::Raw(client);
    let result = stream.upgrade("example.com", false, true, None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_upgrade_with_invalid_server_name_errors() {
    let (client, server) = tokio::io::duplex(64);
    drop(server);
    let mut stream = MaybeTlsStream::Raw(client);
    let err = stream.upgrade("", true, false, None).await.unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn test_upgrade_with_invalid_certs_handshake_fails() {
    // Exercises the accept_invalid_certs (dummy verifier) path.
    let (client, server) = tokio::io::duplex(64);
    // Close the peer so the handshake fails immediately instead of waiting.
    drop(server);
    let mut stream = MaybeTlsStream::Raw(client);
    let result = stream.upgrade("example.com", true, false, None).await;
    assert!(result.is_err());
}
