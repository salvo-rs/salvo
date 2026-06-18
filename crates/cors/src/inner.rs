use std::fmt;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use salvo_core::http::header::HeaderValue;
use salvo_core::{Depot, Request};

type DynFnBool = dyn Fn(Option<&HeaderValue>, &Request, &Depot) -> bool + Send + Sync;
type DynFnBoolAsync = dyn Fn(Option<&HeaderValue>, &Request, &Depot) -> Pin<Box<dyn Future<Output = bool> + Send>>
    + Send
    + Sync;
type DynFnOptHeaderValue =
    dyn Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue> + Send + Sync;
type DynFnOptHeaderValueAsync = dyn Fn(
        Option<&HeaderValue>,
        &Request,
        &Depot,
    ) -> Pin<Box<dyn Future<Output = Option<HeaderValue>> + Send>>
    + Send
    + Sync;

#[derive(Clone, Default)]
pub(crate) enum BoolInner {
    Yes,
    #[default]
    No,
    Dynamic(Arc<DynFnBool>),
    DynamicAsync(Arc<DynFnBoolAsync>),
}

impl BoolInner {
    pub(crate) async fn resolve_async(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> bool {
        match self {
            Self::Yes => true,
            Self::No => false,
            Self::Dynamic(f) => f(origin, req, depot),
            Self::DynamicAsync(f) => f(origin, req, depot).await,
        }
    }
}

impl Debug for BoolInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Yes => f.debug_tuple("Yes").finish(),
            Self::No => f.debug_tuple("No").finish(),
            Self::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            Self::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) enum HeaderInner {
    #[default]
    None,
    Exact(HeaderValue),
    MirrorRequest,
    Dynamic(Arc<DynFnOptHeaderValue>),
    DynamicAsync(Arc<DynFnOptHeaderValueAsync>),
}

impl HeaderInner {
    pub(crate) async fn resolve(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
        mirror: Option<HeaderValue>,
    ) -> Option<HeaderValue> {
        match self {
            Self::None => None,
            Self::Exact(v) => Some(v.clone()),
            Self::MirrorRequest => mirror,
            Self::Dynamic(f) => f(origin, req, depot),
            Self::DynamicAsync(f) => f(origin, req, depot).await,
        }
    }
}

impl Debug for HeaderInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            Self::None => f.debug_tuple("None").finish(),
            Self::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            Self::MirrorRequest => f.debug_tuple("MirrorRequest").finish(),
            Self::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            Self::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) enum HeaderValueInner {
    #[default]
    None,
    Exact(HeaderValue),
    Dynamic(Arc<DynFnOptHeaderValue>),
    DynamicAsync(Arc<DynFnOptHeaderValueAsync>),
}

impl HeaderValueInner {
    pub(crate) async fn resolve(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<HeaderValue> {
        match self {
            Self::None => None,
            Self::Exact(v) => Some(v.clone()),
            Self::Dynamic(f) => f(origin, req, depot),
            Self::DynamicAsync(f) => f(origin, req, depot).await,
        }
    }
}

impl Debug for HeaderValueInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            Self::None => f.debug_tuple("None").finish(),
            Self::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            Self::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            Self::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
        }
    }
}

#[derive(Clone)]
pub(crate) enum HeaderValueListInner {
    Exact(HeaderValue),
    List(Vec<HeaderValue>),
    Dynamic(Arc<DynFnOptHeaderValue>),
    DynamicAsync(Arc<DynFnOptHeaderValueAsync>),
}

impl Default for HeaderValueListInner {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl Debug for HeaderValueListInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            Self::List(inner) => f.debug_tuple("List").field(inner).finish(),
            Self::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            Self::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
        }
    }
}
