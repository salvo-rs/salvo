//! WebTransport streams may arrive before their CONNECT headers are complete.
#![cfg(all(feature = "quinn", feature = "server", feature = "server-handle"))]

use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use salvo_core::conn::quinn::QuinnAcceptor;
use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};
use salvo_core::prelude::*;
use salvo_core::proto::webtransport::server::AcceptedBi;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);

fn varint(value: u64, out: &mut Vec<u8>) {
    if value < 64 {
        out.push(value as u8);
    } else if value < 16_384 {
        out.extend_from_slice(&((value as u16) | 0x4000).to_be_bytes());
    } else {
        out.extend_from_slice(&((value as u32) | 0x8000_0000).to_be_bytes());
    }
}

// QPACK literal field lines without a name reference or Huffman coding.
fn literal(name: &str, value: &str, out: &mut Vec<u8>) {
    if name.len() < 7 {
        out.push(0x20 | name.len() as u8);
    } else {
        out.push(0x27);
        out.push((name.len() - 7) as u8);
    }
    out.extend_from_slice(name.as_bytes());
    out.push(value.len() as u8);
    out.extend_from_slice(value.as_bytes());
}

async fn read_varint(stream: &mut quinn::RecvStream) -> u64 {
    let first = stream.read_u8().await.unwrap();
    let mut value = u64::from(first & 0x3f);
    for _ in 1..(1 << (first >> 6)) {
        value = (value << 8) | u64::from(stream.read_u8().await.unwrap());
    }
    value
}

#[tokio::test]
async fn webtransport_accepts_a_bidi_stream_before_connect_finishes() {
    #[handler]
    async fn endpoint(req: &mut Request) -> Result<(), salvo_core::Error> {
        let session = req.web_transport_mut().await?;
        let Some(AcceptedBi::BidiStream(_, mut stream)) = session.accept_bi().await? else {
            panic!("expected early WebTransport stream");
        };
        let mut payload = Vec::new();
        stream.read_to_end(&mut payload).await?;
        assert_eq!(payload, b"ping");
        stream.write_all(b"pong").await?;
        stream.shutdown().await?;
        Ok(())
    }

    let server_config = RustlsConfig::new(
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_slice())
            .key(include_bytes!("../certs/key.pem").as_slice()),
    )
    .build_quinn_config()
    .unwrap();
    let endpoint_quic =
        quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let address = endpoint_quic.local_addr().unwrap();
    let server = Server::new(QuinnAcceptor::new(
        endpoint_quic,
        address,
        CancellationToken::new(),
    ));
    let handle = server.handle();
    let server_task = tokio::spawn(server.try_serve(Router::new().goal(endpoint)));

    let result = timeout(DEADLINE, async {
        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(include_bytes!("../certs/chain.pem")) {
            roots.add(cert.unwrap()).unwrap();
        }
        let mut tls = rustls::ClientConfig::builder_with_provider(default_crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"h3".to_vec()];
        let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
        )));
        let connection = client
            .connect(address, "testserver.com")
            .unwrap()
            .await
            .unwrap();

        let mut settings = Vec::new();
        for (id, value) in [(0x08, 1), (0x33, 1), (0x2b60_3742, 1), (0x2b60_3743, 1)] {
            varint(id, &mut settings);
            varint(value, &mut settings);
        }
        let mut control_wire = vec![0, 4]; // Control stream type, SETTINGS frame type.
        varint(settings.len() as u64, &mut control_wire);
        control_wire.extend_from_slice(&settings);
        let mut control = connection.open_uni().await.unwrap();
        control.write_all(&control_wire).await.unwrap();

        // Receive the server's SETTINGS before initiating a WebTransport session.
        let mut server_control = connection.accept_uni().await.unwrap();
        assert_eq!(read_varint(&mut server_control).await, 0);
        assert_eq!(read_varint(&mut server_control).await, 4);
        let settings_len = read_varint(&mut server_control).await as usize;
        let mut server_settings = vec![0; settings_len];
        server_control
            .read_exact(&mut server_settings)
            .await
            .unwrap();

        let mut block = vec![0, 0]; // Required Insert Count and Delta Base.
        for (name, value) in [
            (":method", "CONNECT"),
            (":scheme", "https"),
            (":authority", "testserver.com"),
            (":path", "/"),
            (":protocol", "webtransport"),
        ] {
            literal(name, value, &mut block);
        }
        let mut headers = vec![1];
        varint(block.len() as u64, &mut headers);
        headers.extend_from_slice(&block);
        let (mut connect, _connect_response) = connection.open_bi().await.unwrap();
        assert_eq!(connect.id().index(), 0);
        connect
            .write_all(&headers[..headers.len() - 1])
            .await
            .unwrap();

        let (mut early, mut receive) = connection.open_bi().await.unwrap();
        early
            .write_all(&[0x40, 0x41, 0, b'p', b'i', b'n', b'g'])
            .await
            .unwrap();
        early.finish().unwrap();
        // Wait for transport acknowledgement, without sleeps or application timers.
        assert_eq!(early.stopped().await.unwrap(), None);
        connect
            .write_all(&headers[headers.len() - 1..])
            .await
            .unwrap();
        let echoed = receive.read_to_end(16).await.unwrap();
        connection.close(0u32.into(), b"test complete");
        echoed
    })
    .await;
    handle.stop_forceful();
    timeout(DEADLINE, server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.expect("early WebTransport stream stalled"), b"pong");
}

async fn ordinary_request_with_delayed_settings(stalled_first_head: bool) {
    #[handler]
    async fn endpoint() -> &'static str {
        "done"
    }

    let server_config = RustlsConfig::new(
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_slice())
            .key(include_bytes!("../certs/key.pem").as_slice()),
    )
    .build_quinn_config()
    .unwrap();
    let endpoint_quic =
        quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let address = endpoint_quic.local_addr().unwrap();
    let server = Server::new(QuinnAcceptor::new(
        endpoint_quic,
        address,
        CancellationToken::new(),
    ));
    let handle = server.handle();
    let server_task = tokio::spawn(server.try_serve(Router::new().goal(endpoint)));
    let result = timeout(DEADLINE, async {
        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(include_bytes!("../certs/chain.pem")) {
            roots.add(cert.unwrap()).unwrap();
        }
        let mut tls = rustls::ClientConfig::builder_with_provider(default_crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"h3".to_vec()];
        let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
        )));
        let connection = client
            .connect(address, "testserver.com")
            .unwrap()
            .await
            .unwrap();
        let _stalled = if stalled_first_head {
            let (mut send, receive) = connection.open_bi().await.unwrap();
            send.write_all(&[1, 1]).await.unwrap();
            Some((send, receive))
        } else {
            None
        };

        let mut block = vec![0, 0];
        for (name, value) in [
            (":method", "GET"),
            (":scheme", "https"),
            (":authority", "testserver.com"),
            (":path", "/"),
        ] {
            literal(name, value, &mut block);
        }
        let mut headers = vec![1];
        varint(block.len() as u64, &mut headers);
        headers.extend_from_slice(&block);
        let (mut send, mut receive) = connection.open_bi().await.unwrap();
        send.write_all(&headers).await.unwrap();
        send.finish().unwrap();
        assert_eq!(send.stopped().await.unwrap(), None);

        // A late SETTINGS frame must enable concurrent header resolution when
        // the peer confirms that it does not support WebTransport. Keep the
        // critical stream open until the connection closes.
        let _control = if stalled_first_head {
            let mut control = connection.open_uni().await.unwrap();
            control.write_all(&[0, 4, 0]).await.unwrap();
            Some(control)
        } else {
            // The first fully received request must run before SETTINGS arrive.
            None
        };
        let response = receive.read_to_end(4096).await.unwrap();
        connection.close(0u32.into(), b"test complete");
        response
    })
    .await;
    handle.stop_forceful();
    timeout(DEADLINE, server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        result
            .expect("ordinary request waited for SETTINGS or another head")
            .ends_with(b"done")
    );
}

#[tokio::test]
async fn ordinary_h3_request_does_not_wait_for_peer_settings() {
    ordinary_request_with_delayed_settings(false).await;
}

#[tokio::test]
async fn late_peer_settings_enable_independent_h3_headers() {
    ordinary_request_with_delayed_settings(true).await;
}
