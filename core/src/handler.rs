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
/// #[handler]
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
/// #[handler]
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
    #[doc(hidden)]
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
    #[doc(hidden)]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    /// Handle http request.
    #[must_use = "handle future must be used"]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl);
}

// fn print_type_of<T>(_: &T) {
//     println!("{}", std::any::type_name::<T>())
// }
