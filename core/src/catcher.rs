use crate::http::errors::*;
use crate::http::{guess_accept_mime, header, Request, Response, StatusCode};

/// Catch error in current response.
pub trait Catcher: Send + Sync + 'static {
    /// If the current catcher caught the error, it will return true.
    fn catch(&self, req: &Request, res: &mut Response) -> bool;
}

/// Default implementation of Catcher.
pub struct CatcherImpl(HttpError);
impl CatcherImpl {
    /// Create new `CatcherImpl`.
    pub fn new(e: HttpError) -> CatcherImpl {
        CatcherImpl(e)
    }
}
impl Catcher for CatcherImpl {
    fn catch(&self, req: &Request, res: &mut Response) -> bool {
        let status = res.status_code().unwrap_or(StatusCode::NOT_FOUND);
        if status != self.0.code {
            return false;
        }
        let format = guess_accept_mime(req, None);
        let err = if res.http_error.is_some() {
            res.http_error.as_ref().unwrap()
        } else {
            &self.0
        };
        let (format, data) = err.as_bytes(&format);
        res.headers_mut()
            .insert(header::CONTENT_TYPE, format.to_string().parse().unwrap());
        res.write_body_bytes(&data);
        true
    }
}

macro_rules! default_catchers {
    ($($code:expr),+) => (
        let list: Vec<Box<dyn Catcher>> = vec![
        $(
            Box::new(CatcherImpl::new($crate::http::errors::HttpError::from_code($code).unwrap())),
        )+];
        list
    )
}

/// Defaut catchers.
pub mod defaults {
    use super::{Catcher, CatcherImpl};
    use http::status::StatusCode;

    /// Get a new default catchers list.
    pub fn get() -> Vec<Box<dyn Catcher>> {
        default_catchers! {
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::PAYMENT_REQUIRED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::METHOD_NOT_ALLOWED,
            StatusCode::NOT_ACCEPTABLE,
            StatusCode::PROXY_AUTHENTICATION_REQUIRED,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::CONFLICT,
            StatusCode::GONE,
            StatusCode::LENGTH_REQUIRED,
            StatusCode::PRECONDITION_FAILED,
            StatusCode::PAYLOAD_TOO_LARGE,
            StatusCode::URI_TOO_LONG,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            StatusCode::RANGE_NOT_SATISFIABLE,
            StatusCode::EXPECTATION_FAILED,
            StatusCode::IM_A_TEAPOT,
            StatusCode::MISDIRECTED_REQUEST,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::LOCKED,
            StatusCode::FAILED_DEPENDENCY,
            StatusCode::UPGRADE_REQUIRED,
            StatusCode::PRECONDITION_REQUIRED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::NOT_IMPLEMENTED,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            StatusCode::VARIANT_ALSO_NEGOTIATES,
            StatusCode::INSUFFICIENT_STORAGE,
            StatusCode::LOOP_DETECTED,
            StatusCode::NOT_EXTENDED,
            StatusCode::NETWORK_AUTHENTICATION_REQUIRED
        }
    }
}
