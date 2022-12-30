#![allow(dead_code)]
//! Various runtimes for hyper
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread::available_parallelism;
use std::time::{Duration, Instant};

use hyper::rt::{Sleep, Timer};
use tokio::runtime::{self, Runtime};

#[derive(Clone)]
/// An Executor that uses the tokio runtime.
pub struct TokioExecutor;

impl<F> hyper::rt::Executor<F> for TokioExecutor
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn(fut);
    }
}

/// A Timer that uses the tokio runtime.

#[derive(Clone, Debug)]
pub struct TokioTimer;

impl Timer for TokioTimer {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Sleep>> {
        let s = tokio::time::sleep(duration);
        let hs = TokioSleep { inner: Box::pin(s) };
        Box::pin(hs)
    }

    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Sleep>> {
        Box::pin(TokioSleep {
            inner: Box::pin(tokio::time::sleep_until(deadline.into())),
        })
    }
}

struct TokioTimeout<T> {
    inner: Pin<Box<tokio::time::Timeout<T>>>,
}

impl<T> Future for TokioTimeout<T>
where
    T: Future,
{
    type Output = Result<T::Output, tokio::time::error::Elapsed>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(context)
    }
}

// Use TokioSleep to get tokio::time::Sleep to implement Unpin.
// see https://docs.rs/tokio/latest/tokio/time/struct.Sleep.html
pub(crate) struct TokioSleep {
    pub(crate) inner: Pin<Box<tokio::time::Sleep>>,
}

impl Future for TokioSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

// Use HasSleep to get tokio::time::Sleep to implement Unpin.
// see https://docs.rs/tokio/latest/tokio/time/struct.Sleep.html

impl Sleep for TokioSleep {}

#[inline]
fn new_runtime(threads: usize) -> Runtime {
    runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .thread_name("salvo-worker")
        .enable_all()
        .build()
        .unwrap()
}

/// If you don't want to include tokio in your project directly,
/// you can use this function to run server.
///
/// # Example
///
/// ```no_run
/// # use salvo_core::prelude::*;
///
/// #[handler]
/// async fn hello() -> &'static str {
///     "Hello World"
/// }
/// #[tokio::main]
/// async fn main() {
///    let router = Router::new().get(hello);
///    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
///    Server::new(acceptor).serve(router).await;
/// }
/// ```
#[inline]
pub fn run<F: Future>(future: F) {
    run_with_threads(future, available_parallelism().map(|n| n.get()).unwrap_or(1))
}

/// If you don't want to include tokio in your project directly,
/// you can use this function to run server.
///
/// # Example
///
/// ```no_run
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn hello() -> &'static str {
///     "Hello World"
/// }
///
/// fn main() {
///    let router = Router::new().get(hello);
///    salvo_core::runtimes::tokio::run_with_threads(async move {
///         let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
///         Server::new(acceptor).serve(router).await
///    }, 8);
/// }
/// ```
#[inline]
pub fn run_with_threads<F: Future>(future: F, threads: usize) {
    let runtime = new_runtime(threads);
    let _ = runtime.block_on(async { future.await });
}
