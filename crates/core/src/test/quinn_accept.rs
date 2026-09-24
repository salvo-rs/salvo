//! Regression tests for accepting QUIC connections. Endpoints talk over an in-memory network, with
//! tokio's clock paused so that delays are exact. The client pads its ClientHello to span two
//! datagrams, as browsers' post-quantum ClientHellos do.
use std::collections::HashMap;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, ready};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{Instant, sleep, sleep_until, timeout};
use tokio_util::sync::CancellationToken;

use crate::conn::JoinedAcceptor;
use crate::conn::quinn::QuinnAcceptor;
use crate::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider, read_trust_anchor};
use crate::proto::quinn::udp::{RecvMeta, Transmit};
use crate::proto::quinn::{
    self, AsyncUdpSocket, ClientConfig, Endpoint, EndpointConfig, TokioRuntime, UdpPoller,
};
use crate::{Router, Server};

const ONE_WAY: Duration = Duration::from_millis(100);
const SHORT: Duration = Duration::from_millis(10);

/// Decides when the `n`th datagram a socket sends arrives, counting from 0; `None` drops it.
type Fate = fn(usize) -> Option<Duration>;

type Datagram = (SocketAddr, Vec<u8>);

/// In-memory UDP: the inboxes of all sockets, by address.
#[derive(Clone, Debug, Default)]
struct Network(Arc<Mutex<HashMap<SocketAddr, mpsc::UnboundedSender<Datagram>>>>);

#[derive(Debug)]
struct Socket {
    addr: SocketAddr,
    fate: Fate,
    sent: AtomicUsize,
    outbox: mpsc::UnboundedSender<(Instant, Datagram)>,
    inbox: Mutex<mpsc::UnboundedReceiver<Datagram>>,
}

impl Network {
    fn socket(&self, port: u16, fate: Fate) -> Arc<Socket> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let (inbox_tx, inbox) = mpsc::unbounded_channel();
        self.0.lock().unwrap().insert(addr, inbox_tx);
        // Deliver in the order sent: quinn drops packets that arrive before their keys.
        let (outbox, mut queue) = mpsc::unbounded_channel::<(Instant, Datagram)>();
        let network = self.clone();
        tokio::spawn(async move {
            while let Some((due, (to, datagram))) = queue.recv().await {
                sleep_until(due).await;
                if let Some(inbox) = network.0.lock().unwrap().get(&to) {
                    let _ = inbox.send((addr, datagram));
                }
            }
        });
        Arc::new(Socket {
            addr,
            fate,
            sent: AtomicUsize::new(0),
            outbox,
            inbox: Mutex::new(inbox),
        })
    }
}

impl AsyncUdpSocket for Socket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(AlwaysWritable)
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        let n = self.sent.fetch_add(1, Ordering::SeqCst);
        if let Some(delay) = (self.fate)(n) {
            let datagram = (transmit.destination, transmit.contents.to_vec());
            let _ = self.outbox.send((Instant::now() + delay, datagram));
        }
        Ok(())
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let Some((from, datagram)) = ready!(self.inbox.lock().unwrap().poll_recv(cx)) else {
            return Poll::Pending;
        };
        bufs[0][..datagram.len()].copy_from_slice(&datagram);
        meta[0] = RecvMeta {
            addr: from,
            len: datagram.len(),
            stride: datagram.len(),
            ..RecvMeta::default()
        };
        Poll::Ready(Ok(1))
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

#[derive(Debug)]
struct AlwaysWritable;
impl UdpPoller for AlwaysWritable {
    fn poll_writable(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn endpoint(socket: Arc<Socket>, server: Option<quinn::ServerConfig>) -> Endpoint {
    let runtime = Arc::new(TokioRuntime);
    Endpoint::new_with_abstract_socket(EndpointConfig::default(), server, socket, runtime).unwrap()
}

fn acceptor(network: &Network, port: u16, fate: Fate) -> QuinnAcceptor {
    let cert = include_bytes!("../../certs/cert.pem").as_slice();
    let key = include_bytes!("../../certs/key.pem").as_slice();
    let config = RustlsConfig::new(Keycert::new().cert(cert).key(key));
    let socket = network.socket(port, fate);
    let endpoint = endpoint(socket.clone(), Some(config.try_into().unwrap()));
    QuinnAcceptor::new(endpoint, socket.addr, CancellationToken::new())
}

fn client(network: &Network, port: u16, fate: Fate) -> Endpoint {
    let roots = read_trust_anchor(include_bytes!("../../certs/chain.pem")).unwrap();
    let mut tls = rustls::ClientConfig::builder_with_provider(default_crypto_provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    // Enough ALPN padding to split the ClientHello over two datagrams.
    tls.alpn_protocols = (0..60)
        .map(|i| format!("padding-protocol-{i:03}").into_bytes())
        .chain([b"h3".to_vec()])
        .collect();
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap();
    let mut endpoint = endpoint(network.socket(port, fate), None);
    endpoint.set_default_client_config(ClientConfig::new(Arc::new(crypto)));
    endpoint
}

async fn connect(endpoint: &Endpoint, server_port: u16) -> quinn::Connection {
    let server = SocketAddr::from(([127, 0, 0, 1], server_port));
    let connecting = endpoint.connect(server, "localhost").unwrap();
    timeout(Duration::from_secs(2), connecting)
        .await
        .expect("QUIC handshake should complete")
        .unwrap()
}

/// Wait for the server's first HTTP/3 stream, which it opens to send its SETTINGS.
async fn h3_stream(conn: &quinn::Connection, limit: Duration) {
    let stream = timeout(limit, conn.accept_uni()).await;
    stream.expect("HTTP/3 should start in time").unwrap();
}

#[tokio::test(start_paused = true)]
async fn joined_accept_does_not_drop_quic_handshake() {
    let network = Network::default();
    let acceptor = JoinedAcceptor::new(
        acceptor(&network, 443, |_| Some(ONE_WAY)),
        acceptor(&network, 444, |_| Some(SHORT)),
    );
    tokio::spawn(Server::new(acceptor).serve(Router::new()));
    let first = client(&network, 1, |_| Some(ONE_WAY));
    let first = tokio::spawn(async move { connect(&first, 443).await });
    // A connection to the other listener completes its handshake while the first one's is in
    // progress.
    sleep(ONE_WAY * 3 / 2).await;
    connect(&client(&network, 2, |_| Some(SHORT)), 444).await;
    h3_stream(&first.await.unwrap(), ONE_WAY * 3).await;
}

#[tokio::test(start_paused = true)]
async fn stalled_handshake_does_not_block_other_connections() {
    let network = Network::default();
    tokio::spawn(Server::new(acceptor(&network, 443, |_| Some(ONE_WAY))).serve(Router::new()));
    // Deliver only the ClientHello, so this handshake never completes.
    let stalled = client(&network, 1, |n| (n < 2).then_some(ONE_WAY));
    let _stalled = stalled.connect(([127, 0, 0, 1], 443).into(), "localhost");
    sleep(ONE_WAY * 3).await;
    let conn = connect(&client(&network, 2, |_| Some(ONE_WAY)), 443).await;
    h3_stream(&conn, ONE_WAY * 3).await;
}
