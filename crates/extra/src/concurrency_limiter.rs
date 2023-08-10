//! concurrency limiter middleware

use tokio::sync::Semaphore;

use salvo_core::http::StatusError;
use salvo_core::http::{Request, Response};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// MaxConcurrency
pub struct MaxConcurrency {
    semaphore: Semaphore,
}
#[async_trait]
impl Handler for MaxConcurrency {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        match self.semaphore.acquire().await {
            Ok(_) => {
                ctrl.call_next(req, depot, res).await;
            }
            Err(e) => {
                tracing::error!(
                    "Max concurrency semaphore is never closed, acquire should never fail: {}",
                    e
                );
                res.render(StatusError::payload_too_large().brief("Max concurrency reached."));
            }
        }
    }
}
/// Create a new `MaxConcurrency`.
#[inline]
pub fn max_concurrency(size: usize) -> MaxConcurrency {
    MaxConcurrency {
        semaphore: Semaphore::new(size),
    }
}
