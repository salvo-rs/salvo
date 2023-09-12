//! `Handler` is used for handle [`Request`].
//!
//! * `Handler` can be used as middleware to handle [`Request`].
//!
//! # Example
//!
//! ```
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn middleware() {
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     Router::new().hoop(middleware);
//! }
//! ```
//!
//! * `Handler` can be used as endpoint to handle [`Request`].
//!
//! # Example
//!
//! ```
//! # use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn middleware() {
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     Router::new().goal(middleware);
//! }
//! ```

use crate::http::StatusCode;
use crate::{async_trait, Depot, FlowCtrl, Request, Response};

/// Handler
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

#[doc(hidden)]
pub struct EmptyHandler;
#[async_trait]
impl Handler for EmptyHandler {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        res.status_code(StatusCode::OK);
    }
}

/// This is a empty implement for `Handler`.
///
/// `EmptyHandler` does nothing except set [`Response`]'s satus as [`StatusCode::OK`], it just marker a router exits.
pub fn empty() -> EmptyHandler {
    EmptyHandler
}

#[doc(hidden)]
#[non_exhaustive]
pub struct WhenHoop<H, F> {
    pub inner: H,
    pub filter: F,
}
#[async_trait]
impl<H, F> Handler for WhenHoop<H, F>
where
    H: Handler,
    F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if (self.filter)(req, depot) {
            self.inner.handle(req, depot, res, ctrl).await;
        }
    }
}

/// `Skipper` is used in many middlewares.
pub trait Skipper: Send + Sync + 'static {
    /// Check if the request should be skipped.
    fn skipped(&self, req: &mut Request, depot: &Depot) -> bool;
}
impl<F> Skipper for F
where
    F: Fn(&mut Request, &Depot) -> bool + Send + Sync + 'static,
{
    fn skipped(&self, req: &mut Request, depot: &Depot) -> bool {
        (self)(req, depot)
    }
}

/// `none_skipper` will skipper nothing.
///
/// It can be used as default `Skipper` in middleware.
pub fn none_skipper(_req: &mut Request, _depot: &Depot) -> bool {
    false
}

macro_rules! handler_tuple_impls {
    ($(
        $Tuple:tt {
            $(($idx:tt) -> $T:ident,)+
        }
    )+) => {$(
        #[async_trait::async_trait]
        impl<$($T,)+> Handler for ($($T,)+) where $($T: Handler,)+
        {
            async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
                $(
                    if !res.is_stamped() {
                        self.$idx.handle(req, depot, res, ctrl).await;
                    }
                )+
            }
        })+
    }
}
macro_rules! skipper_tuple_impls {
    ($(
        $Tuple:tt {
            $(($idx:tt) -> $T:ident,)+
        }
    )+) => {$(
        impl<$($T,)+> Skipper for ($($T,)+) where $($T: Skipper,)+
        {
            fn skipped(&self, req: &mut Request, depot: &Depot) -> bool {
                $(
                    if self.$idx.skipped(req, depot) {
                        return true;
                    }
                )+
                false
            }
        })+
    }
}

macro_rules! __for_each_tuple {
    ($callback:ident) => {
        $callback! {
            1 {
                (0) -> A,
            }
            2 {
                (0) -> A,
                (1) -> B,
            }
            3 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
            }
            4 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
            }
            5 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
            }
            6 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
            }
            7 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
            }
            8 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
            }
        }
    };
}

__for_each_tuple!(handler_tuple_impls);
__for_each_tuple!(skipper_tuple_impls);
