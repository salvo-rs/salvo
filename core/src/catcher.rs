use http::status::StatusCode;
use crate::http::{header, Response, Request, guess_accept_mime};
use crate::http::errors::*;

pub trait Catcher: Send + Sync + 'static {
    fn catch(&self, req: &Request, resp: &mut Response)->bool;
}

pub struct CatcherImpl(Box<dyn HttpError>);
impl CatcherImpl{
    pub fn new(e: Box<dyn HttpError>) -> CatcherImpl{
        CatcherImpl(e)
    }
}
impl Catcher for CatcherImpl {
    fn catch(&self, req: &Request, resp: &mut Response)->bool {
        let status = resp.status_code().unwrap_or(StatusCode::NOT_FOUND);
        if status != self.0.code() {
            return false;
        }
        let format = guess_accept_mime(req, None);
        let err = if resp.http_error.is_some() {
            resp.http_error.as_ref().unwrap()
        } else {
            &self.0
        };
        let (format, data) = err.as_bytes(&format);
        resp.headers_mut().insert(header::CONTENT_TYPE, format.to_string().parse().unwrap());
        resp.body_writers.push(Box::new(data));
        true
    }
}

macro_rules! default_catchers {
    ($($name:ty),+) => (
        let mut list: Vec<Box<dyn Catcher>> = vec![];
        $(
            list.push(Box::new(CatcherImpl::new(Box::new(<$name>::with_default()))));
        )+
        list
    )
}

pub mod defaults {
    use super::{Catcher, CatcherImpl};
    use crate::http::errors::http_error::*;

    pub fn get() -> Vec<Box<dyn Catcher>> {
        default_catchers! {
            BadRequestError,       
            UnauthorizedError,     
            PaymentRequiredError,  
            ForbiddenError,        
            NotFoundError,         
            MethodNotAllowedError, 
            NotAcceptableError,    
            ProxyAuthenticationRequiredError,
            RequestTimeoutError,       
            ConflictError,             
            GoneError,                 
            LengthRequiredError,       
            PreconditionFailedError,   
            PayloadTooLargeError,      
            UriTooLongError,           
            UnsupportedMediaTypeError, 
            RangeNotSatisfiableError,  
            ExpectationFailedError,    
            ImATeapotError,            
            MisdirectedRequestError,   
            UnprocessableEntityError,  
            LockedError,               
            FailedDependencyError,     
            UpgradeRequiredError,      
            PreconditionRequiredError, 
            TooManyRequestsError,      
            RequestHeaderFieldsTooLargeError,
            UnavailableForLegalReasonsError, 
            InternalServerError,    
            NotImplementedError,    
            BadGatewayError,        
            ServiceUnavailableError,
            GatewayTimeoutError,    
            HttpVersionNotSupportedError,
            VariantAlsoNegotiatesError,
            InsufficientStorageError,  
            LoopDetectedError,         
            NotExtendedError,          
            NetworkAuthenticationRequiredError
        }
    }
}

