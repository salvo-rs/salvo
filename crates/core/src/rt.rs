//! runtime module.
//!
//! Only supports tokio runtime in current version.
//! More runtimes will be supported in the future.

pub use hyper::rt::*;

/// Tokio runtimes
pub mod tokio {
    pub use salvo_utils::rt::{TokioExecutor, TokioIo};

    /// If you don't want to include tokio in your project directly,
    /// you can use this function to run server.
    pub fn start<F: std::future::Future>(future: F) {
        #[cfg(not(feature = "io-uring"))]
        {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
                .thread_name("salvo-worker")
                .enable_all()
                .build()
                .unwrap()
                .block_on(future);
        }

        #[cfg(feature = "io-uring")]
        tokio_uring::start(future);
    }
}
