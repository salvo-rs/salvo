use async_trait::async_trait;

use crate::http::{Request, Response};
use crate::routing::FlowCtrl;
use crate::Depot;

/// `Handler` is used for handle [`Request`].
///
/// * `Handler` can be used as middleware to handle [`Request`].
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
///
/// #[fn_handler]
/// async fn middleware() {
/// }
///
/// #[tokio::main]
/// async fn main() {
///     Router::new().hoop(middleware);
/// }
/// ```
///
/// * `Handler` can be used as endpoint to handle [`Request`].
///
/// # Example
///
/// ```
/// # use salvo_core::prelude::*;
///
/// #[fn_handler]
/// async fn middleware() {
/// }
///
/// #[tokio::main]
/// async fn main() {
///     Router::new().handle(middleware);
/// }
/// ```
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    /// Handle http request.
    #[must_use = "handle future must be used"]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl);
}
