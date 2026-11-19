//! AWS Lambda HTTP runtime adapter.
//!
//! [`LambdaServer`] wraps a Salvo [`Service`] and converts between
//! `lambda_http` events and Salvo requests/responses.

mod request;
mod response;

use std::future::Future;

use salvo_core::http::ResBody;
use salvo_core::{Request, Response, Router, Service};

use crate::server::GeneralServer;

pub use self::request::LambdaRequestExt;

/// A Salvo service adapted to the AWS Lambda HTTP runtime.
#[derive(Debug)]
pub struct LambdaServer {
    service: Service,
}

impl Clone for LambdaServer {
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

impl LambdaServer {
    /// Creates a Lambda server from a [`Service`].
    #[must_use]
    pub fn new(service: Service) -> Self {
        Self { service }
    }

    /// Handles one Lambda HTTP request, buffering the response body.
    ///
    /// The request is converted to a Salvo request, dispatched through the
    /// service, and the resulting response is buffered into a single Lambda
    /// body. Prefer [`LambdaServer::handle_streaming`] when the response body
    /// may be large or unbounded.
    ///
    /// # Errors
    ///
    /// Returns a [`lambda_http::Error`] when the response body cannot be read
    /// or converted.
    pub async fn handle(
        &self,
        req: lambda_http::Request,
    ) -> Result<lambda_http::Response<lambda_http::Body>, lambda_http::Error> {
        let salvo_request = request::into_salvo_request(req);
        let salvo_response = self.service.serve(salvo_request).await;
        Ok(response::from_salvo_response(salvo_response).await)
    }

    /// Handles one Lambda HTTP request, streaming the response body.
    ///
    /// The response body is forwarded to the Lambda runtime as a stream of
    /// chunks without being buffered in memory. Pass this handler to
    /// `lambda_http::run_with_streaming_response` instead of
    /// `lambda_http::run`:
    ///
    /// ```ignore
    /// lambda_http::run_with_streaming_response(lambda_http::service_fn(|req| {
    ///     let server = server.clone();
    ///     async move { server.handle_streaming(req).await }
    /// }))
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a [`lambda_http::Error`] when the response body cannot be read
    /// or converted.
    pub async fn handle_streaming(
        &self,
        req: lambda_http::Request,
    ) -> Result<lambda_http::Response<ResBody>, lambda_http::Error> {
        let salvo_request = request::into_salvo_request(req);
        let salvo_response = self.service.serve(salvo_request).await;
        Ok(response::from_salvo_response_streaming(salvo_response))
    }
}

impl From<Service> for LambdaServer {
    fn from(service: Service) -> Self {
        Self::new(service)
    }
}

impl From<Router> for LambdaServer {
    fn from(router: Router) -> Self {
        Self::new(Service::new(router))
    }
}

impl GeneralServer for LambdaServer {
    fn serve(&self, request: Request) -> impl Future<Output = Response> {
        self.service.serve(request)
    }
}
