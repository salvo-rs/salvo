#![allow(missing_docs, clippy::unwrap_used)]

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use salvo_core::conn::{Acceptor, ConnCtrl};
use salvo_core::prelude::*;
use salvo_core::server::ServerHandle;
use salvo_extra::websocket::WebSocketUpgrade;
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, client_async};

const WAIT: Duration = Duration::from_secs(3);

#[derive(Default, Debug)]
struct State {
    active: AtomicUsize,
    changed: Notify,
    ctrl: Mutex<Option<ConnCtrl>>,
}

struct Active(Arc<State>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        self.0.changed.notify_waiters();
    }
}

#[derive(Debug)]
struct Endpoint {
    state: Arc<State>,
    parked: bool,
}

#[async_trait]
impl Handler for Endpoint {
    async fn handle(&self, req: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        *self.state.ctrl.lock().unwrap() = req.extensions().get::<ConnCtrl>().cloned();
        let state = self.state.clone();
        let parked = self.parked;
        WebSocketUpgrade::new()
            .upgrade(req, res, move |mut socket| async move {
                let _active = Active(state.clone());
                state.active.fetch_add(1, Ordering::SeqCst);
                state.changed.notify_waiters();
                if parked {
                    std::future::pending::<()>().await;
                }
                while let Some(Ok(message)) = socket.recv().await {
                    if socket.send(message).await.is_err() {
                        break;
                    }
                }
            })
            .await
            .unwrap();
    }
}

async fn wait_active(state: &State, count: usize) {
    timeout(WAIT, async {
        loop {
            let changed = state.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if state.active.load(Ordering::SeqCst) == count {
                return;
            }
            changed.await;
        }
    })
    .await
    .expect("upgraded callback count did not reach its expected value");
}

struct TestServer {
    address: SocketAddr,
    handle: ServerHandle,
    task: JoinHandle<io::Result<()>>,
}

async fn serve(router: Router, max: Option<usize>) -> TestServer {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let address = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let mut server = Server::new(acceptor);
    if let Some(max) = max {
        server = server.max_connections(max);
    }
    let handle = server.handle();
    let task = tokio::spawn(server.try_serve(router));
    TestServer {
        address,
        handle,
        task,
    }
}

#[derive(Clone, Copy, Debug)]
enum Backend {
    WebSocket,
    #[cfg(feature = "hyper-client")]
    Hyper,
    #[cfg(feature = "reqwest-client")]
    Reqwest,
}

fn backends() -> Vec<Backend> {
    vec![
        Backend::WebSocket,
        #[cfg(feature = "hyper-client")]
        Backend::Hyper,
        #[cfg(feature = "reqwest-client")]
        Backend::Reqwest,
    ]
}

struct Setup {
    front: TestServer,
    upstream: Option<TestServer>,
    state: Arc<State>,
}

impl Setup {
    async fn new(backend: Backend, parked: bool) -> Self {
        let state = Arc::new(State::default());
        let router = Router::new().goal(Endpoint {
            state: state.clone(),
            parked,
        });
        if matches!(backend, Backend::WebSocket) {
            return Self {
                front: serve(router, Some(1)).await,
                upstream: None,
                state,
            };
        }
        let upstream = serve(router, None).await;
        let url = format!("http://{}", upstream.address);
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let router = match backend {
            Backend::WebSocket => unreachable!(),
            #[cfg(feature = "hyper-client")]
            Backend::Hyper => Router::new().goal(salvo_proxy::Proxy::use_hyper_client(vec![url])),
            #[cfg(feature = "reqwest-client")]
            Backend::Reqwest => Router::new().goal(salvo_proxy::Proxy::new(
                vec![url],
                salvo_proxy::ReqwestClient::new(
                    reqwest::Client::builder().no_proxy().build().unwrap(),
                ),
            )),
        };
        Self {
            front: serve(router, Some(1)).await,
            upstream: Some(upstream),
            state,
        }
    }

    async fn finish_upstream(self) {
        if let Some(upstream) = self.upstream {
            upstream.handle.stop_forceful();
            timeout(WAIT, upstream.task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }
}

async fn connect(address: SocketAddr) -> WebSocketStream<TcpStream> {
    let stream = TcpStream::connect(address).await.unwrap();
    let (socket, response) = client_async(format!("ws://{address}/"), stream)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    socket
}

async fn echo(socket: &mut WebSocketStream<TcpStream>) {
    socket.send(Message::Text("alive".into())).await.unwrap();
    let message = timeout(WAIT, socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(message.into_text().unwrap(), "alive");
}

#[tokio::test]
async fn upgraded_connections_retain_capacity_until_the_socket_closes() {
    for backend in backends() {
        let mut setup = Setup::new(backend, false).await;
        let first = timeout(WAIT, connect(setup.front.address)).await.unwrap();
        wait_active(&setup.state, 1).await;
        let address = setup.front.address;
        let mut second = tokio::spawn(async move { connect(address).await });
        assert!(
            timeout(Duration::from_millis(50), &mut second)
                .await
                .is_err(),
            "{backend:?} released the physical connection permit during upgrade"
        );
        drop(first);
        let mut second = timeout(WAIT, second).await.unwrap().unwrap();
        echo(&mut second).await;
        drop(second);
        wait_active(&setup.state, 0).await;
        setup.front.handle.stop_graceful(None);
        timeout(WAIT, &mut setup.front.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        setup.finish_upstream().await;
    }
}

#[tokio::test]
async fn graceful_shutdown_drains_upgraded_connections() {
    for backend in backends() {
        let mut setup = Setup::new(backend, false).await;
        let mut socket = timeout(WAIT, connect(setup.front.address)).await.unwrap();
        wait_active(&setup.state, 1).await;
        setup.front.handle.stop_graceful(None);
        assert!(
            timeout(Duration::from_millis(50), &mut setup.front.task)
                .await
                .is_err()
        );
        echo(&mut socket).await;
        drop(socket);
        timeout(WAIT, &mut setup.front.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        wait_active(&setup.state, 0).await;
        setup.finish_upstream().await;
    }
}

#[derive(Clone, Copy)]
enum Stop {
    Forceful,
    GracefulTimeout,
    DropServingFuture,
}

#[tokio::test]
async fn stopping_a_server_cancels_websockets_and_proxy_tunnels() {
    for backend in backends() {
        for stop in [
            Stop::Forceful,
            Stop::GracefulTimeout,
            Stop::DropServingFuture,
        ] {
            let mut setup = Setup::new(backend, false).await;
            let mut socket = timeout(WAIT, connect(setup.front.address)).await.unwrap();
            wait_active(&setup.state, 1).await;
            echo(&mut socket).await;
            match stop {
                Stop::Forceful => setup.front.handle.stop_forceful(),
                Stop::GracefulTimeout => {
                    setup.front.handle.stop_graceful(Duration::from_millis(20))
                }
                Stop::DropServingFuture => setup.front.task.abort(),
            }
            let stopped = timeout(WAIT, &mut setup.front.task).await.unwrap();
            if matches!(stop, Stop::DropServingFuture) {
                assert!(stopped.unwrap_err().is_cancelled());
            } else {
                stopped.unwrap().unwrap();
            }
            wait_active(&setup.state, 0).await;
            let closed = timeout(WAIT, socket.next()).await.unwrap();
            assert!(closed.is_none() || closed.unwrap().is_err());
            setup.finish_upstream().await;
        }
    }
}

#[tokio::test]
async fn abort_and_forceful_stop_cancel_a_callback_that_never_polls_its_socket() {
    for abort_connection in [true, false] {
        let mut setup = Setup::new(Backend::WebSocket, true).await;
        let mut socket = timeout(WAIT, connect(setup.front.address)).await.unwrap();
        wait_active(&setup.state, 1).await;
        if abort_connection {
            setup.state.ctrl.lock().unwrap().as_ref().unwrap().abort();
        } else {
            setup.front.handle.stop_forceful();
            timeout(WAIT, &mut setup.front.task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
        wait_active(&setup.state, 0).await;
        let closed = timeout(WAIT, socket.next()).await.unwrap();
        assert!(closed.is_none() || closed.unwrap().is_err());
        if abort_connection {
            setup.front.handle.stop_forceful();
            timeout(WAIT, &mut setup.front.task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
        setup.finish_upstream().await;
    }
}
