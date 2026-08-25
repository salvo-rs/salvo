use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use rcgen::{CertificateParams, KeyPair};
use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;
use salvo::proto::webtransport;
use sha2::{Digest, Sha256};
use time::{Duration as TimeDuration, OffsetDateTime};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::pin;

macro_rules! log_result {
    ($expr:expr) => {
        if let Err(err) = $expr {
            tracing::error!("{err:?}");
        }
    };
}

#[derive(Debug)]
struct CertificateHash([u8; 32]);

#[handler]
impl CertificateHash {
    async fn handle(&self) -> Json<[u8; 32]> {
        Json(self.0)
    }
}

fn certificate_params(now: OffsetDateTime) -> Result<CertificateParams> {
    let mut params = CertificateParams::new(vec!["localhost".to_owned(), "127.0.0.1".to_owned()])?;
    // WebTransport permits certificate hashes only for short-lived certificates. Backdating by
    // one minute tolerates small clock differences while keeping the total lifetime under 14 days.
    params.not_before = now - TimeDuration::minutes(1);
    params.not_after = now + TimeDuration::days(13);
    Ok(params)
}

fn generate_certificate() -> Result<(RustlsConfig, CertificateHash)> {
    let params = certificate_params(OffsetDateTime::now_utc())?;
    let signing_key = KeyPair::generate()?;
    let certificate = params.self_signed(&signing_key)?;
    let certificate_hash = Sha256::digest(certificate.der().as_ref()).into();
    let keycert = Keycert::new()
        .cert(certificate.pem().into_bytes())
        .key(signing_key.serialize_pem().into_bytes());

    Ok((
        RustlsConfig::new(keycert),
        CertificateHash(certificate_hash),
    ))
}

async fn echo_stream<T, R>(send: T, recv: R) -> anyhow::Result<()>
where
    T: AsyncWrite,
    R: AsyncRead,
{
    pin!(send);
    pin!(recv);

    tracing::info!("Got stream");
    let mut buf = Vec::new();
    recv.read_to_end(&mut buf).await?;

    let message = Bytes::from(buf);
    send_chunked(send, message).await?;

    Ok(())
}
// Used to test that all chunks arrive properly as it is easy to write an impl which only reads and
// writes the first chunk.
async fn send_chunked(mut send: impl AsyncWrite + Unpin, data: Bytes) -> anyhow::Result<()> {
    for chunk in data.chunks(4) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        tracing::info!("Sending {chunk:?}");
        send.write_all(chunk).await?;
    }

    Ok(())
}

#[handler]
async fn connect(req: &mut Request) -> Result<(), salvo::Error> {
    let session = req.web_transport_mut().await.unwrap();
    let session_id = session.session_id();

    // This will open a bidirectional stream and send a message to the client right after
    // connecting!
    let stream = session.open_bi(session_id).await?;
    let mut datagram_reader = session.datagram_reader();
    let mut datagram_sender = session.datagram_sender();

    tokio::spawn(async move {
        log_result!(open_bidi_test(stream).await);
    });
    loop {
        tokio::select! {
            datagram = datagram_reader.read_datagram() => {
                let datagram = match datagram {
                    Ok(datagram) => datagram,
                    Err(e) => {
                        tracing::error!("Failed to read datagram: {e:?}");
                        break;
                    }
                };
                tracing::info!("Received datagram: {datagram:?}");
                let datagram = datagram.into_payload();
                datagram_sender.send_datagram(datagram)?;
            }
            uni_stream = session.accept_uni() => {
                let (id, stream) = uni_stream?.unwrap();

                let send = session.open_uni(id).await?;
                tokio::spawn( async move { log_result!(echo_stream(send, stream).await); });
            }
            stream = session.accept_bi() => {
                if let Some(webtransport::server::AcceptedBi::BidiStream(_, stream)) = stream? {
                    let (send, recv) = salvo::proto::quic::BidiStream::split(stream);
                    tokio::spawn( async move { log_result!(echo_stream(send, recv).await); });
                }
            }
            else => {
                break
            }
        }
    }

    tracing::info!("Finished handling session");

    Ok(())
}

async fn open_bidi_test<S>(mut stream: S) -> anyhow::Result<()>
where
    S: Unpin + AsyncRead + AsyncWrite,
{
    tracing::info!("Opening bidirectional stream");

    stream
        .write_all(b"Hello from a server initiated bidi stream")
        .await
        .context("Failed to respond")?;

    let mut resp = Vec::new();
    stream.shutdown().await?;
    stream.read_to_end(&mut resp).await?;

    tracing::info!("Got response from client: {resp:?}");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let (config, certificate_hash) = generate_certificate()?;

    let router = Router::new()
        .push(Router::with_path("counter").goal(connect))
        .push(Router::with_path("certificate-hash").get(certificate_hash))
        .push(
            Router::with_path("{*path}")
                .get(StaticDir::new(["webtransport/static", "./static"]).defaults("client.html")),
        );

    let listener = TcpListener::new(("0.0.0.0", 8698)).rustls(config.clone());

    let acceptor = QuinnListener::new(config, ("0.0.0.0", 8698))
        .join(listener)
        .bind()
        .await;

    Server::new(acceptor).serve(router).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_parameters_meet_webtransport_requirements() {
        let now = OffsetDateTime::now_utc();
        let params = certificate_params(now).unwrap();

        assert!(params.not_before <= now);
        assert!(params.not_after >= now);
        assert!(params.not_after - params.not_before < TimeDuration::days(14));
    }

    #[test]
    fn generated_certificate_builds_a_quinn_config() {
        let (config, certificate_hash) = generate_certificate().unwrap();

        assert_ne!(certificate_hash.0, [0; 32]);
        config.build_quinn_config().unwrap();
    }
}
