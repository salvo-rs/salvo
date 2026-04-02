use std::collections::HashMap;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, OnceLock};
use std::thread;

use bytes::Bytes;
use tokio::sync::{mpsc as tokio_mpsc, oneshot};
use tokio_uring::fs::File as UringFile;

use super::{ChunkFuture, ChunkRead};

#[derive(Debug)]
pub(crate) struct IoUringFile {
    runtime: Arc<UringRuntime>,
    handle: u64,
}

impl IoUringFile {
    pub(crate) async fn from_std(file: File) -> io::Result<Self> {
        let runtime = shared_runtime()?;
        let handle = runtime.open(file).await?;
        Ok(Self { runtime, handle })
    }
}

impl ChunkRead for IoUringFile {
    fn read_chunk(self, offset: u64, max_bytes: usize) -> ChunkFuture<Self> {
        let runtime = self.runtime.clone();
        let handle = self.handle;
        Box::pin(async move {
            let bytes = runtime.read_chunk(handle, offset, max_bytes).await?;
            Ok((self, bytes))
        })
    }
}

impl Drop for IoUringFile {
    fn drop(&mut self) {
        self.runtime.close(self.handle);
    }
}

#[derive(Debug)]
struct UringRuntime {
    sender: tokio_mpsc::UnboundedSender<WorkerRequest>,
}

impl UringRuntime {
    fn start() -> io::Result<Self> {
        let (sender, receiver) = tokio_mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("salvo-io-uring-fs".to_owned())
            .spawn(move || worker_loop(receiver, ready_tx))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { sender }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(io::Error::new(
                ErrorKind::BrokenPipe,
                "io_uring worker failed to initialize",
            )),
        }
    }

    async fn open(&self, file: File) -> io::Result<u64> {
        let (response, rx) = oneshot::channel();
        self.sender
            .send(WorkerRequest::Open { file, response })
            .map_err(|_| io::Error::new(ErrorKind::BrokenPipe, "io_uring worker is unavailable"))?;

        rx.await.map_err(|_| {
            io::Error::new(
                ErrorKind::BrokenPipe,
                "io_uring worker dropped the open response",
            )
        })?
    }

    async fn read_chunk(&self, handle: u64, offset: u64, max_bytes: usize) -> io::Result<Bytes> {
        let (response, rx) = oneshot::channel();
        self.sender
            .send(WorkerRequest::Read {
                handle,
                offset,
                max_bytes,
                response,
            })
            .map_err(|_| io::Error::new(ErrorKind::BrokenPipe, "io_uring worker is unavailable"))?;

        rx.await.map_err(|_| {
            io::Error::new(
                ErrorKind::BrokenPipe,
                "io_uring worker dropped the read response",
            )
        })?
    }

    fn close(&self, handle: u64) {
        let _ = self.sender.send(WorkerRequest::Close { handle });
    }
}

#[derive(Clone, Debug)]
struct RuntimeInitError {
    kind: ErrorKind,
    message: String,
}

impl RuntimeInitError {
    fn into_io_error(self) -> io::Error {
        io::Error::new(self.kind, self.message)
    }
}

impl From<io::Error> for RuntimeInitError {
    fn from(value: io::Error) -> Self {
        Self {
            kind: value.kind(),
            message: value.to_string(),
        }
    }
}

static RUNTIME: OnceLock<Result<Arc<UringRuntime>, RuntimeInitError>> = OnceLock::new();

fn shared_runtime() -> io::Result<Arc<UringRuntime>> {
    match RUNTIME.get_or_init(|| UringRuntime::start().map(Arc::new).map_err(Into::into)) {
        Ok(runtime) => Ok(runtime.clone()),
        Err(err) => Err(err.clone().into_io_error()),
    }
}

#[derive(Debug)]
enum WorkerRequest {
    Open {
        file: File,
        response: oneshot::Sender<io::Result<u64>>,
    },
    Read {
        handle: u64,
        offset: u64,
        max_bytes: usize,
        response: oneshot::Sender<io::Result<Bytes>>,
    },
    Close {
        handle: u64,
    },
}

fn worker_loop(
    receiver: tokio_mpsc::UnboundedReceiver<WorkerRequest>,
    ready_tx: SyncSender<io::Result<()>>,
) {
    let worker = async move {
        let _ = ready_tx.send(Ok(()));
        run_worker(receiver).await;
    };

    tokio_uring::start(worker);
}

async fn run_worker(mut receiver: tokio_mpsc::UnboundedReceiver<WorkerRequest>) {
    let mut next_handle = 1u64;
    let mut files = HashMap::<u64, UringFile>::new();

    while let Some(request) = receiver.recv().await {
        match request {
            WorkerRequest::Open { file, response } => {
                let handle = next_handle;
                next_handle += 1;
                files.insert(handle, UringFile::from_std(file));
                let _ = response.send(Ok(handle));
            }
            WorkerRequest::Read {
                handle,
                offset,
                max_bytes,
                response,
            } => {
                let result = if let Some(file) = files.get(&handle) {
                    let (result, mut buf) = file.read_at(vec![0u8; max_bytes], offset).await;
                    match result {
                        Ok(0) => Err(ErrorKind::UnexpectedEof.into()),
                        Ok(bytes) => {
                            buf.truncate(bytes);
                            Ok(Bytes::from(buf))
                        }
                        Err(err) => Err(err),
                    }
                } else {
                    Err(io::Error::new(
                        ErrorKind::NotFound,
                        "io_uring file handle is no longer available",
                    ))
                };
                let _ = response.send(result);
            }
            WorkerRequest::Close { handle } => {
                if let Some(file) = files.remove(&handle) {
                    let _ = file.close().await;
                }
            }
        }
    }

    for (_, file) in files.drain() {
        let _ = file.close().await;
    }
}
