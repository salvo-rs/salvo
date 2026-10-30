use std::sync::Arc;

use salvo_core::http::{HeaderValue, Method, StatusCode};
use salvo_core::routing::{Filter, FilterInfo, PathState};
use salvo_core::{Depot, Request, Response, Router, async_trait, handler};

use crate::handlers::apply_common_headers;
use crate::utils::check_tus_version;
use crate::{H_TUS_RESUMABLE, H_TUS_VERSION, ProtocolError, TUS_VERSION, Tus, TusError};

const HEADER: &str = "x-http-method-override";

fn effective_method(req: &Request) -> Result<Method, ()> {
    let mut values = req.headers().get_all(HEADER).iter();
    let Some(value) = values.next() else {
        return Ok(req.method().clone());
    };
    if values.next().is_some() {
        return Err(());
    }
    Method::from_bytes(value.as_bytes()).map_err(|_| ())
}

// Normalize only when the entire endpoint path matches. Routing runs before
// hoops, so access-control hoops and the handler see the same effective method.
// A failed match must not change the request seen by sibling application routes.
#[derive(Debug)]
pub(crate) struct TusMethodFilter(pub(crate) Method);

#[async_trait]
impl Filter for TusMethodFilter {
    async fn filter(&self, req: &mut Request, state: &mut PathState<'_>) -> bool {
        if !state.is_ended() {
            return false;
        }
        let Ok(method) = effective_method(req) else {
            return false;
        };
        if method != self.0 {
            return false;
        }
        *req.method_mut() = method;
        true
    }

    fn info(&self) -> FilterInfo {
        FilterInfo::Method(self.0.clone())
    }

    fn collect_methods(&self, methods: &mut Vec<Method>) {
        methods.push(self.0.clone());
    }

    async fn filter_for_method(
        &self,
        _req: &mut Request,
        state: &mut PathState<'_>,
        method: &Method,
    ) -> Option<bool> {
        Some(state.is_ended() && method == self.0)
    }
}

#[derive(Debug)]
struct InvalidOverride;

#[async_trait]
impl Filter for InvalidOverride {
    async fn filter(&self, req: &mut Request, state: &mut PathState<'_>) -> bool {
        state.is_ended() && effective_method(req).is_err()
    }
}

#[handler]
async fn invalid_override(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get_typed::<Arc<Tus>>().expect("missing tus state");
    apply_common_headers(req, &state.options, &mut res.headers);
    if let Err(error) = check_tus_version(
        req.headers()
            .get(H_TUS_RESUMABLE)
            .and_then(|value| value.to_str().ok()),
    ) {
        if matches!(error, ProtocolError::UnsupportedTusVersion(_)) {
            res.headers
                .insert(H_TUS_VERSION, HeaderValue::from_static(TUS_VERSION));
        }
        res.status_code(TusError::Protocol(error).status());
        return;
    }
    res.status_code(StatusCode::BAD_REQUEST);
}

pub(crate) fn invalid_override_router() -> Router {
    Router::new()
        .push(Router::new().filter(InvalidOverride).goal(invalid_override))
        .push(
            Router::with_path("{id}")
                .filter(InvalidOverride)
                .goal(invalid_override),
        )
}
