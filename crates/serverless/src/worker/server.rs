//! Cloudflare Workers server adapter.

use std::future::Future;
use std::sync::Arc;

use salvo_core::{Request, Response, Router, Service};

use super::{request, response};
use crate::server::GeneralServer;

/// A Salvo service adapted to the Cloudflare Workers runtime.
#[derive(Debug)]
pub struct WorkerServer {
    service: Service,
}

impl Clone for WorkerServer {
    fn clone(&self) -> Self {
        let mut service = Service::new(self.service.router.clone());
        service.catcher.clone_from(&self.service.catcher);
        service.hoops.clone_from(&self.service.hoops);
        service
            .allowed_media_types
            .clone_from(&self.service.allowed_media_types);
        Self { service }
    }
}

impl WorkerServer {
    /// Creates a Workers server from a [`Service`].
    #[must_use]
    pub fn new(service: Service) -> Self {
        Self { service }
    }

    /// Handles one Workers HTTP request.
    ///
    /// The request is converted to a Salvo request, dispatched through the
    /// service, and the resulting response is buffered into a Workers body.
    ///
    /// # Errors
    ///
    /// Returns a [`worker::Result`] error when the request body cannot be
    /// read, the websocket handshake fails, or the Workers response body
    /// stream cannot be built.
    pub async fn handle(
        &self,
        req: worker::HttpRequest,
        env: worker::Env,
        ctx: worker::Context,
    ) -> worker::Result<worker::HttpResponse> {
        let ctx = Arc::new(ctx);
        let salvo_request = request::into_salvo_request(req, &env, &ctx).await?;
        let salvo_response = self.service.serve(salvo_request).await;
        response::into_worker_response(salvo_response)
    }
}

impl From<Service> for WorkerServer {
    fn from(service: Service) -> Self {
        Self::new(service)
    }
}

impl From<Router> for WorkerServer {
    fn from(router: Router) -> Self {
        Self::new(Service::new(router))
    }
}

impl GeneralServer for WorkerServer {
    fn serve(&self, request: Request) -> impl Future<Output = Response> {
        self.service.serve(request)
    }
}
