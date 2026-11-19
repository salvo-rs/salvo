//! Transport-independent server abstraction shared by runtime adapters.

use std::future::Future;

use salvo_core::{Request, Response, Service};

/// A server that dispatches a Salvo [`Request`] without a network transport.
///
/// Runtime adapters implement this trait so generated entry points can treat
/// every runtime the same way.
pub trait GeneralServer {
    /// Serves `request` and returns the produced [`Response`].
    fn serve(&self, request: Request) -> impl Future<Output = Response>;
}

impl GeneralServer for Service {
    fn serve(&self, request: Request) -> impl Future<Output = Response> {
        Self::serve(self, request)
    }
}
