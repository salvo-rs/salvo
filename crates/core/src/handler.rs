//! Handler module for handle [`Request`].
//!
//! Middleware is actually also a `Handler`. They can do some processing before or after the request reaches the `Handler` that officially handles the request, such as: login verification, data compression, etc.
//!
//! Middleware is added through the `hoop` function of `Router`. The added middleware will affect the current `Router` and all its internal descendants `Router`.
//!
//! ## Macro `#[handler]`
//!
//! `#[handler]` can greatly simplify the writing of the code, and improve the flexibility of the code.
//!
//! It can be added to a function to make it implement `Handler`:
//!
//! ```
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "hello world!"
//! }
//! ````
//!
//! This is equivalent to:
//!
//! ```
//! use salvo_core::prelude::*;
//!
//! #[allow(non_camel_case_types)]
//! struct hello;
//!
//! #[async_trait]
//! impl Handler for hello {
//!     async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
//!         res.render(Text::Plain("hello world!"));
//!     }
//! }
//! ````
//!
//! As you can see, in the case of using `#[handler]`, the code becomes much simpler:
//! - No need to manually add `#[async_trait]`.
//! - The parameters that are not needed in the function have been omitted, and the required parameters can be arranged in any order.
//! - For objects that implement `Writer` or `Scribe` abstraction, it can be directly used as the return value of the function. Here `&'static str` implements `Scribe`, so it can be returned directly as the return value of the function.
//!
//! `#[handler]` can not only be added to the function, but also can be added to the `impl` of `struct` to let `struct` implement `Handler`. At this time, the `handle` function in the `impl` code block will be Identified as the specific implementation of `handle` in `Handler`:
//!
//! ```
//! use salvo_core::prelude::*;
//!
//! struct Hello;
//!
//! #[handler]
//! impl Hello {
//!     async fn handle(&self, res: &mut Response) {
//!         res.render(Text::Plain("hello world!"));
//!     }
//! }
//! ````
//!
//! ## Handle errors
//!
//! `Handler` in Salvo can return `Result`, only the types of `Ok` and `Err` in `Result` are implemented `Writer` trait.
//!
//! Taking into account the widespread use of `anyhow`, the `Writer` implementation of `anyhow::Error` is provided by
//! default if `anyhow` feature is enabled, and `anyhow::Error` is Mapped to `InternalServerError`.
//!
//! For custom error types, you can output different error pages according to your needs.
//!
//! ```ignore
//! use anyhow::anyhow;
//! use salvo_core::prelude::*;
//!
//! struct CustomError;
//! #[async_trait]
//! impl Writer for CustomError {
//!     async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
//!         res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
//!         res.render("custom error");
//!     }
//! }
//!
//! #[handler]
//! async fn handle_anyhow() -> Result<(), anyhow::Error> {
//!     Err(anyhow::anyhow!("anyhow error"))
//! }
//! #[handler]
//! async fn handle_custom() -> Result<(), CustomError> {
//!     Err(CustomError)
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new()
//!         .push(Router::new().path("anyhow").get(handle_anyhow))
//!         .push(Router::new().path("custom").get(handle_custom));
//!     let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
//!
//! ## Implement Handler trait directly
//!
//! Under certain circumstances, We need to implment `Handler` direclty.
//!
//! ```
//! use salvo_core::prelude::*;
//!  use crate::salvo_core::http::Body;
//!
//! pub struct MaxSizeHandler(u64);
//! #[async_trait]
//! impl Handler for MaxSizeHandler {
//!     async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
//!         if let Some(upper) = req.body().size_hint().upper() {
//!             if upper > self.0 {
//!                 res.render(StatusError::payload_too_large());
//!                 ctrl.skip_rest();
//!             }
//!         }
//!     }
//! }
//! ```
use std::sync::Arc;

use crate::http::StatusCode;
use crate::{Depot, FlowCtrl, Request, Response, async_trait};

/// `Handler` is used for handle [`Request`].
///
/// View [module level documentation](index.html) for more details.
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
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    );

    /// Wrap to `ArcHandler`.
    #[inline]
    fn arc(self) -> ArcHandler
    where
        Self: Sized,
    {
        ArcHandler(Arc::new(self))
    }

    /// Wrap to `HoopedHandler`.
    #[inline]
    fn hooped<H: Handler>(self) -> HoopedHandler
    where
        Self: Sized,
    {
        HoopedHandler::new(self)
    }

    /// Hoop this handler with middleware.
    #[inline]
    fn hoop<H: Handler>(self, hoop: H) -> HoopedHandler
    where
        Self: Sized,
    {
        HoopedHandler::new(self).hoop(hoop)
    }

    /// Hoop this handler with middleware.
    ///
    /// This middleware is only effective when the filter returns true..
    #[inline]
    fn hoop_when<H, F>(self, hoop: H, filter: F) -> HoopedHandler
    where
        Self: Sized,
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        HoopedHandler::new(self).hoop_when(hoop, filter)
    }
}

/// A handler that wraps another [Handler] to enable it to be cloneable.
#[derive(Clone)]
pub struct ArcHandler(Arc<dyn Handler>);

#[async_trait]
impl Handler for ArcHandler {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        self.0.handle(req, depot, res, ctrl).await
    }
}

#[doc(hidden)]
pub struct EmptyHandler;
#[async_trait]
impl Handler for EmptyHandler {
    async fn handle(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
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
impl<H, F> WhenHoop<H, F> {
    pub fn new(inner: H, filter: F) -> Self {
        Self { inner, filter }
    }
}
#[async_trait]
impl<H, F> Handler for WhenHoop<H, F>
where
    H: Handler,
    F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if (self.filter)(req, depot) {
            self.inner.handle(req, depot, res, ctrl).await;
        }
    }
}

/// `Skipper` is used to check if the request should be skipped.
///
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

/// Handler that wrap [`Handler`] to let it use middlwares.
#[non_exhaustive]
pub struct HoopedHandler {
    inner: Arc<dyn Handler>,
    hoops: Vec<Arc<dyn Handler>>,
}
impl HoopedHandler {
    /// Create new `HoopedHandler`.
    pub fn new<H: Handler>(inner: H) -> Self {
        Self {
            inner: Arc::new(inner),
            hoops: vec![],
        }
    }

    /// Get current catcher's middlewares reference.
    #[inline]
    pub fn hoops(&self) -> &Vec<Arc<dyn Handler>> {
        &self.hoops
    }
    /// Get current catcher's middlewares mutable reference.
    #[inline]
    pub fn hoops_mut(&mut self) -> &mut Vec<Arc<dyn Handler>> {
        &mut self.hoops
    }

    /// Add a handler as middleware, it will run the handler when error catched.
    #[inline]
    pub fn hoop<H: Handler>(mut self, hoop: H) -> Self {
        self.hoops.push(Arc::new(hoop));
        self
    }

    /// Add a handler as middleware, it will run the handler when error catched.
    ///
    /// This middleware is only effective when the filter returns true..
    #[inline]
    pub fn hoop_when<H, F>(mut self, hoop: H, filter: F) -> Self
    where
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        self.hoops.push(Arc::new(WhenHoop::new(hoop, filter)));
        self
    }
}
#[async_trait]
impl Handler for HoopedHandler {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let inner: Arc<dyn Handler> = self.inner.clone();
        let right = ctrl.handlers.split_off(ctrl.cursor);
        ctrl.handlers.append(
            &mut self
                .hoops
                .iter()
                .cloned()
                .chain([inner])
                .chain(right)
                .collect(),
        );
        ctrl.call_next(req, depot, res).await;
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

crate::for_each_tuple!(handler_tuple_impls);
crate::for_each_tuple!(skipper_tuple_impls);
