use std::time::Duration;

use anyhow::{Context, Result};
use bytes::{BufMut, Bytes, BytesMut};
use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;
use salvo::proto::webtransport;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::pin;

macro_rules! log_result {
    ($expr:expr) => {
        if let Err(err) = $expr {
            tracing::error!("{err:?}");
        }
    };
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

    // This will open a bidirectional stream and send a message to the client right after connecting!
    let stream = session.open_bi(session_id).await?;

    tokio::spawn(async move {
        log_result!(open_bidi_test(stream).await);
    });
    loop {
        tokio::select! {
            datagram = session.accept_datagram() => {
                let datagram = datagram?;
                if let Some((_, datagram)) = datagram {
                    tracing::info!("Responding with {datagram:?}");
                    // Put something before to make sure encoding and decoding works and don't just
                    // pass through
                    let mut resp = BytesMut::from(&b"Response: "[..]);
                    resp.put(datagram);

                    session.send_datagram(resp.freeze())?;
                    tracing::info!("Finished sending datagram");
                }
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
async fn main() {
    tracing_subscriber::fmt().init();

    let cert = include_bytes!("../certs/cert.pem").to_vec();
    let key = include_bytes!("../certs/key.pem").to_vec();

    let router = Router::new()
        .push(Router::with_path("counter").goal(connect))
        .push(
            Router::with_path("{*path}")
                .get(StaticDir::new(["webtransport/static", "./static"]).defaults("client.html")),
        );

    let config = RustlsConfig::new(Keycert::new().cert(cert.as_slice()).key(key.as_slice()));
    let listener = TcpListener::new(("0.0.0.0", 5800)).rustls(config.clone());

    let acceptor = QuinnListener::new(config, ("0.0.0.0", 5800))
        .join(listener)
        .bind()
        .await;

    Server::new(acceptor).serve(router).await;
}
