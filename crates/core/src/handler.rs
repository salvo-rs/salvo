//! Handler abstractions for processing [`Request`] values.
//!
//! A middleware is also a [`Handler`]. Middleware can inspect or modify the request,
//! share state through [`Depot`], write to [`Response`], or stop the remaining handler
//! chain with [`FlowCtrl::skip_rest`].
//!
//! Middleware is added with [`Router::hoop`](crate::routing::Router::hoop).
//! Middleware attached to a router applies to that router and all of its descendants.
//!
//! ## Macro `#[handler]`
//!
//! `#[handler]` keeps handlers concise while still allowing Salvo to inject any
//! request context the function asks for.
//!
//! Add it to a function to make that function implement [`Handler`]:
//!
//! ```
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "hello world!"
//! }
//! ```
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
//!     async fn handle(
//!         &self,
//!         _req: &mut Request,
//!         _depot: &mut Depot,
//!         res: &mut Response,
//!         _ctrl: &mut FlowCtrl,
//!     ) {
//!         res.render(Text::Plain("hello world!"));
//!     }
//! }
//! ```
//!
//! With `#[handler]`, the code becomes much simpler:
//!
//! - No need to manually add `#[async_trait]`.
//! - Unused context parameters can be omitted.
//! - Required parameters can be listed in any supported order.
//! - Return values that implement [`Writer`](crate::writing::Writer) or
//!   [`Scribe`](crate::writing::Scribe) can be returned directly.
//!
//! `#[handler]` can also be added to an `impl` block. In that form, the `handle`
//! method becomes the [`Handler::handle`] implementation for the struct:
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
//! ```
//!
//! ## Handle errors
//!
//! A Salvo handler can return `Result<T, E>` when both `T` and `E` can be written
//! to the response.
//!
//! When the `anyhow` feature is enabled, `anyhow::Error` can be returned from a
//! handler and is rendered as `500 Internal Server Error`.
//!
//! Custom error types can implement [`Writer`](crate::writing::Writer) to control the generated
//! response:
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
//!     let acceptor = TcpListener::new("127.0.0.1:8698").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
//!
//! ## Implement Handler trait directly
//!
//! Implement [`Handler`] directly when a type needs to own configuration or when
//! the handler logic cannot be expressed cleanly as a function.
//!
//! ```
//! use salvo_core::hyper::body::Body;
//! use salvo_core::prelude::*;
//!
//! pub struct MaxSizeHandler(u64);
//!
//! #[async_trait]
//! impl Handler for MaxSizeHandler {
//!     async fn handle(
//!         &self,
//!         req: &mut Request,
//!         _depot: &mut Depot,
//!         res: &mut Response,
//!         ctrl: &mut FlowCtrl,
//!     ) {
//!         if let Some(upper) = req.body().size_hint().upper() {
//!             if upper > self.0 {
//!                 res.render(StatusError::payload_too_large());
//!                 ctrl.skip_rest();
//!             }
//!         }
//!     }
//! }
//! ```
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use crate::http::StatusCode;
use crate::{Depot, FlowCtrl, Request, Response, async_trait};

/// Processes a request and writes to a response.
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
    /// Handles one HTTP request.
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

    /// Wraps this handler in a [`HoopedHandler`].
    #[inline]
    fn hooped(self) -> HoopedHandler
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
    /// This middleware is only effective when the filter returns `true`.
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
impl Debug for ArcHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArcHandler")
            .field("inner", &self.0.type_name())
            .finish()
    }
}

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
#[derive(Debug)]
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

/// An empty implementation of `Handler`.
///
/// `EmptyHandler` does nothing except setting the [`Response`] status to [`StatusCode::OK`]; it
/// just marks the end of a handler chain when no handler is set.
#[must_use]
pub fn empty() -> EmptyHandler {
    EmptyHandler
}

#[doc(hidden)]
#[non_exhaustive]
pub struct WhenHoop<H, F> {
    pub inner: H,
    pub filter: F,
}

impl<H: Debug, F: Debug> Debug for WhenHoop<H, F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("WhenHoop")
            .field("inner", &self.inner)
            .field("filter", &self.filter)
            .finish()
    }
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
        } else {
            ctrl.call_next(req, depot, res).await;
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
        self(req, depot)
    }
}

/// Handler that wrap [`Handler`] to let it use middlewares.
#[non_exhaustive]
pub struct HoopedHandler {
    inner: Arc<dyn Handler>,
    hoops: Vec<Arc<dyn Handler>>,
}

impl Clone for HoopedHandler {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            hoops: self.hoops.clone(),
        }
    }
}

impl Debug for HoopedHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HoopedHandler")
            .field("inner", &self.inner.type_name())
            .field("hoops.len", &self.hoops.len())
            .finish()
    }
}

impl HoopedHandler {
    /// Creates a new `HoopedHandler`.
    pub fn new<H: Handler>(inner: H) -> Self {
        Self {
            inner: Arc::new(inner),
            hoops: vec![],
        }
    }

    /// Get a reference to the middlewares attached to this handler.
    #[inline]
    #[must_use]
    pub fn hoops(&self) -> &Vec<Arc<dyn Handler>> {
        &self.hoops
    }
    /// Get a mutable reference to the middlewares attached to this handler.
    #[inline]
    pub fn hoops_mut(&mut self) -> &mut Vec<Arc<dyn Handler>> {
        &mut self.hoops
    }

    /// Add a handler as middleware. It will run before this handler.
    #[inline]
    #[must_use]
    pub fn hoop<H: Handler>(mut self, hoop: H) -> Self {
        self.hoops.push(Arc::new(hoop));
        self
    }

    /// Add a handler as middleware. It runs this middleware only when the filter returns `true`.
    #[inline]
    #[must_use]
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
        ctrl.handlers.extend(
            self.hoops
                .iter()
                .cloned()
                .chain([inner])
                .map(Some)
                .chain(right),
        );
        ctrl.call_next(req, depot, res).await;
    }
}

/// `none_skipper` skips nothing.
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

#[cfg(test)]
mod tests {
    use salvo_macros::handler;

    use super::*;
    use crate::Response;
    use crate::http::StatusCode;
    use crate::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_empty_handler() {
        let res = TestClient::get("http://127.0.0.1:8698/")
            .send(empty())
            .await;
        assert_eq!(res.status_code, Some(StatusCode::OK));
    }

    #[tokio::test]
    async fn test_arc_handler() {
        #[handler]
        async fn hello(res: &mut Response) {
            res.status_code(StatusCode::OK);
            res.render("hello");
        }
        let mut res = TestClient::get("http://127.0.0.1:8698/")
            .send(hello.arc())
            .await;
        assert_eq!(res.status_code, Some(StatusCode::OK));
        assert_eq!(res.take_string().await.unwrap(), "hello");
    }

    #[test]
    fn test_hooped_handler_without_type_parameter() {
        #[handler]
        async fn hello(res: &mut Response) {
            res.status_code(StatusCode::OK);
            res.render("hello");
        }

        let _handler: HoopedHandler = hello.hooped();
    }
}
