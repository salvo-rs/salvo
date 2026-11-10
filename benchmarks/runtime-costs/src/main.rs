use std::sync::Arc;
use std::time::Duration;

use salvo_core::conn::Acceptor;
use salvo_core::extract::Extractible;
use salvo_core::fuse::{FuseAction, FuseConfig, FuseInfo, FusePolicy};
use salvo_core::http::ReqBody;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_extra::tower_compat::TowerLayerCompat;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

mod support;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Query {
    value: String,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Generic<T> {
    #[salvo(extract(flatten))]
    inner: T,
}

#[handler]
async fn hello(res: &mut Response) -> &'static str {
    res.headers_mut()
        .insert("content-length", "2".parse().unwrap());
    "ok"
}

#[derive(Debug)]
struct Policy(bool);
#[async_trait]
impl FusePolicy for Policy {
    async fn decide(&self, _: &FuseInfo) -> FuseAction {
        if self.0 {
            tokio::task::yield_now().await;
        }
        FuseAction::Accept(FuseConfig::disabled())
    }
}

#[handler]
async fn extracted(req: &mut Request, res: &mut Response) -> &'static str {
    let data: Generic<Query> = req.extract(&mut Depot::new()).await.unwrap();
    assert_eq!(data.inner.value, "expected");
    res.headers_mut()
        .insert("content-length", "2".parse().unwrap());
    "ok"
}

fn application() -> Router {
    Router::new()
        .get(hello)
        .push(Router::with_path("generic").get(extracted))
        .push(
            Router::with_path("tower")
                .hoop(tower::layer::util::Identity::new().compat::<ReqBody>())
                .get(hello),
        )
}

async fn response(socket: &mut TcpStream, close: bool, path: &str) {
    let connection = if close { "Connection: close\r\n" } else { "" };
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{connection}\r\n");
    socket.write_all(request.as_bytes()).await.unwrap();
    let mut received = Vec::with_capacity(256);
    let mut scratch = [0; 1024];
    let end = loop {
        let count = socket.read(&mut scratch).await.unwrap();
        assert!(count > 0);
        received.extend_from_slice(&scratch[..count]);
        if let Some(position) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
        assert!(received.len() < 8192);
    };
    let head = std::str::from_utf8(&received[..end]).unwrap();
    assert!(head.starts_with("HTTP/1.1 200"));
    assert!(
        head.lines()
            .any(|line| line.eq_ignore_ascii_case("content-length: 2"))
    );
    while received.len() < end + 2 {
        let count = socket.read(&mut scratch).await.unwrap();
        assert!(count > 0);
        received.extend_from_slice(&scratch[..count]);
    }
    assert_eq!(&received[end..], b"ok");
    if close {
        assert_eq!(socket.read(&mut [0; 1]).await.unwrap(), 0);
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    runtime.block_on(async {
        let data: Generic<Query> = TestClient::get("http://localhost/?value=expected")
            .build()
            .extract(&mut Depot::new())
            .await
            .unwrap();
        assert_eq!(data.inner.value, "expected");
    });
    support::header();
    support::measure(&runtime, "metadata/plain_batch1000", || async {
        for _ in 0..1000 {
            std::hint::black_box(<Query as Extractible>::metadata());
        }
    });
    support::measure(&runtime, "metadata/generic_batch1000", || async {
        for _ in 0..1000 {
            std::hint::black_box(<Generic<Query> as Extractible>::metadata());
        }
    });
    for generic in [false, true] {
        support::measure(
            &runtime,
            if generic {
                "metadata/generic_threads4_batch40000"
            } else {
                "metadata/plain_threads4_batch40000"
            },
            || async {
                std::thread::scope(|scope| {
                    for _ in 0..4 {
                        scope.spawn(move || {
                            for _ in 0..10_000 {
                                if generic {
                                    std::hint::black_box(
                                        <Generic<Query> as Extractible>::metadata(),
                                    );
                                } else {
                                    std::hint::black_box(<Query as Extractible>::metadata());
                                }
                            }
                        });
                    }
                });
            },
        );
    }
    for tower in [false, true] {
        let router = Router::new().get(hello);
        let service = Service::new(if tower {
            router.hoop(tower::layer::util::Identity::new().compat::<ReqBody>())
        } else {
            router
        });
        support::measure(
            &runtime,
            if tower {
                "service/tower_identity"
            } else {
                "service/plain"
            },
            || async {
                let mut res = TestClient::get("http://localhost/").send(&service).await;
                assert_eq!(res.status_code, Some(StatusCode::OK));
                assert_eq!(res.take_string().await.unwrap(), "ok");
            },
        );
    }
    for policy in ["none", "immediate", "yielding"] {
        let listener = runtime.block_on(TcpListener::new("127.0.0.1:0").bind());
        let address = listener.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = match policy {
            "none" => Server::new(listener).disable_fuse(),
            "immediate" => Server::new(listener).fuse_policy(Policy(false)),
            _ => Server::new(listener).fuse_policy(Policy(true)),
        };
        let handle = server.handle();
        let task = runtime.spawn(server.try_serve(application()));
        support::measure(&runtime, &format!("tcp/{policy}/fresh"), || async {
            timeout(Duration::from_secs(5), async {
                let mut socket = TcpStream::connect(address).await.unwrap();
                socket.set_nodelay(true).unwrap();
                response(&mut socket, true, "/").await;
            })
            .await
            .unwrap();
        });
        let socket = runtime.block_on(TcpStream::connect(address)).unwrap();
        socket.set_nodelay(true).unwrap();
        let socket = Arc::new(tokio::sync::Mutex::new(socket));
        support::measure(&runtime, &format!("tcp/{policy}/keep_alive"), || async {
            let mut socket = socket.lock().await;
            timeout(Duration::from_secs(5), response(&mut socket, false, "/"))
                .await
                .unwrap();
        });
        let mut persistent = Vec::new();
        for _ in 0..4 {
            let stream = runtime.block_on(TcpStream::connect(address)).unwrap();
            stream.set_nodelay(true).unwrap();
            persistent.push(Arc::new(tokio::sync::Mutex::new(stream)));
        }
        support::measure(&runtime, &format!("tcp/{policy}/mixed_batch8"), || async {
            let mut tasks = tokio::task::JoinSet::new();
            for index in 0..8 {
                let stream = persistent[index % 4].clone();
                tasks.spawn(async move {
                    let path = match index % 3 {
                        0 => "/",
                        1 => "/generic?value=expected",
                        _ => "/tower",
                    };
                    timeout(Duration::from_secs(5), async {
                        if index < 4 {
                            response(&mut *stream.lock().await, false, path).await;
                        } else {
                            let mut stream = TcpStream::connect(address).await.unwrap();
                            stream.set_nodelay(true).unwrap();
                            response(&mut stream, true, path).await;
                        }
                    })
                    .await
                    .unwrap();
                });
            }
            while let Some(result) = tasks.join_next().await {
                result.unwrap();
            }
        });
        drop(persistent);
        drop(socket);
        handle.stop_graceful(Some(Duration::from_secs(1)));
        runtime
            .block_on(timeout(Duration::from_secs(5), task))
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
