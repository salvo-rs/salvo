//! A simple logging middleware.
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::logging::Logger;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().get(hello);
//!     let service = Service::new(router).hoop(Logger::new());
//!
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     Server::new(acceptor).serve(service).await;
//! }
//! ```
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use salvo_core::http::{Request, ResBody, Response, StatusCode};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};
use tracing::{Instrument, Level};

/// A simple logger middleware.
///
/// Within a Service, the response status and duration are recorded after final
/// media-type checks, catchers and HEAD handling. The duration ends before body
/// transmission and does not measure streaming or network completion.
#[derive(Debug)]
pub struct Logger {
    /// Whether to log status error.
    ///
    /// If true, the logger will try log [`StatusError`][salvo_core::http::StatusError] information
    /// if response body is [`ResBody::Error`].
    ///
    /// **Note**: If you have handled the error before logging and the body is not
    /// [`ResBody::Error`], the error information cannot be recorded.
    pub log_status_error: bool,
}
impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}
impl Logger {
    /// Creates a new `Logger` middleware.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            log_status_error: true,
        }
    }

    /// Set whether to log [`StatusError`][salvo_core::http::StatusError] information if response
    /// body is [`ResBody::Error`].
    ///
    /// **Note**: If you have handled the error before logging and the body is not
    /// [`ResBody::Error`], the error information cannot be recorded.
    #[must_use]
    pub fn log_status_error(mut self, log_status_error: bool) -> Self {
        self.log_status_error = log_status_error;
        self
    }
}

#[async_trait]
impl Handler for Logger {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let span = tracing::span!(
            Level::INFO,
            "Request",
            remote_addr = %req.remote_addr(),
            version = ?req.version(),
            method = %req.method(),
            // Log only the path, not the full URI: the query string can carry
            // secrets (`?token=`, `?api_key=`, signed URLs) that should not land
            // in logs by default.
            path = %req.uri().path(),
        );

        async move {
            let now = Instant::now();
            let error_details = Arc::new(OnceLock::new());
            let captured_error = error_details.clone();
            let log_status_error = self.log_status_error;
            let span = tracing::Span::current();
            // Register before calling downstream: an adapter may replace or retain
            // the Request, but Service still owns the final response observers.
            let fallback = req
                .try_on_response(move |res| {
                    span.in_scope(|| {
                        log_response(
                            res,
                            now,
                            log_status_error,
                            captured_error.get().map(String::as_str),
                        );
                    });
                })
                .err();
            ctrl.call_next(req, depot, res).await;
            // A catcher normally consumes StatusError before finalization. Keep
            // the configured diagnostic details while observing the final status.
            if self.log_status_error
                && let ResBody::Error(error) = &res.body
            {
                let _ = error_details.set(format!("{error:?}"));
            }
            if let Some(observer) = fallback {
                observer(res);
            }
        }
        .instrument(span)
        .await
    }
}

fn log_response(res: &Response, started: Instant, log_status_error: bool, error: Option<&str>) {
    let duration = started.elapsed();
    let status = res.status_code.unwrap_or(match &res.body {
        ResBody::None => StatusCode::NOT_FOUND,
        ResBody::Error(error) => error.code,
        _ => StatusCode::OK,
    });
    if log_status_error && let ResBody::Error(error) = &res.body {
        tracing::info!(%status, ?duration, ?error, "Response");
    } else if let Some(error) = error.filter(|_| log_status_error) {
        tracing::info!(%status, ?duration, error = %error, "Response");
    } else {
        tracing::info!(%status, ?duration, "Response");
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::catcher::Catcher;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use tracing_test::traced_test;

    use super::*;

    #[tokio::test]
    #[traced_test]
    async fn logs_final_media_type_rejection_status() {
        #[handler]
        async fn json(res: &mut Response) {
            res.render(Json("value"));
        }
        let service = Service::new(Router::new().get(json))
            .hoop(Logger::new())
            .allowed_media_types(vec!["text/plain".parse().unwrap()]);
        let response = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
        assert!(logs_contain("status=415"));
        assert!(!logs_contain("status=200"));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_custom_catcher_final_status() {
        #[handler]
        async fn failed() -> StatusError {
            StatusError::internal_server_error()
        }
        #[handler]
        async fn recover(res: &mut Response, ctrl: &mut FlowCtrl) {
            res.status_code(StatusCode::ACCEPTED);
            res.render("recovered");
            ctrl.skip_rest();
        }
        let service = Service::new(Router::new().get(failed))
            .hoop(Logger::new())
            .catcher(Catcher::default().hoop(recover));
        let response = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::ACCEPTED));
        assert!(logs_contain("status=202"));
        assert!(!logs_contain("status=500"));
    }

    #[tokio::test]
    #[traced_test]
    async fn logs_when_called_as_standalone_handler() {
        let mut req = TestClient::get("http://localhost/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::default();
        Logger::new()
            .handle(&mut req, &mut depot, &mut res, &mut ctrl)
            .await;
        assert!(logs_contain("Response"));
        assert!(logs_contain("duration"));
    }

    #[tokio::test]
    #[traced_test]
    async fn cancelled_service_does_not_log_a_completed_response() {
        use std::future::poll_fn;
        use std::task::Poll;

        #[handler]
        async fn pending() {
            std::future::pending::<()>().await;
        }
        let service = Service::new(Router::new().get(pending)).hoop(Logger::new());
        let mut request = Box::pin(service.handle(TestClient::get("http://localhost/").build()));
        poll_fn(|cx| {
            assert!(request.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(request);
        assert!(!logs_contain("Response"));
    }

    #[cfg(feature = "tower-compat")]
    #[tokio::test]
    #[traced_test]
    async fn tower_short_circuit_records_the_final_response_once() {
        use http_body_util::Full;
        use salvo_core::http::ReqBody;
        use salvo_core::hyper::body::Bytes;
        use salvo_core::hyper::{self};

        use crate::tower_compat::TowerLayerCompat;

        let layer = tower::layer::layer_fn(|_inner| {
            tower::service_fn(|_req: hyper::Request<ReqBody>| async {
                Ok::<_, std::io::Error>(hyper::Response::new(Full::new(Bytes::from_static(
                    b"rejected",
                ))))
            })
        });
        #[handler]
        async fn unused() {}
        let service = Service::new(Router::new().hoop(layer.compat::<ReqBody>()).goal(unused))
            .hoop(Logger::new());
        let _ = TestClient::get("http://localhost/").send(&service).await;
        logs_assert(|lines| {
            let count = lines
                .iter()
                .filter(|line| line.contains("Response status="))
                .count();
            if count == 1 {
                Ok(())
            } else {
                Err(format!("expected one final response event, got {count}"))
            }
        });
    }

    #[tokio::test]
    #[traced_test]
    async fn disabled_status_error_omits_error_details() {
        #[handler]
        async fn failed() -> StatusError {
            StatusError::internal_server_error().brief("private-disabled-error-marker")
        }
        let router = Router::with_path("failure")
            .hoop(Logger::new().log_status_error(false))
            .goal(failed);
        let response = TestClient::get("http://localhost/failure")
            .send(router)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert!(logs_contain("duration"));
        assert!(!logs_contain("private-disabled-error-marker"));
    }

    #[tokio::test]
    #[traced_test]
    async fn enabled_status_error_includes_error_details() {
        #[handler]
        async fn failed() -> StatusError {
            StatusError::internal_server_error().brief("private-enabled-error-marker")
        }
        let router = Router::with_path("failure")
            .hoop(Logger::new().log_status_error(true))
            .goal(failed);
        TestClient::get("http://localhost/failure")
            .send(router)
            .await;
        assert!(logs_contain("private-enabled-error-marker"));
    }

    #[tokio::test]
    #[traced_test]
    async fn test_log() {
        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(Logger::new())
            .push(Router::with_path("hello").get(hello));

        TestClient::get("http://127.0.0.1:5801/hello")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(logs_contain("duration"));
    }
}
