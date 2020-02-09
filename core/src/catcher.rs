use http::status::StatusCode;
use mime::Mime;
use crate::http::{header, Response, Request};
use crate::http::errors::*;

pub trait Catcher: Send + Sync + 'static {
    fn catch(&self, req: &Request, resp: &mut Response)->bool;
}

fn error_html(e: &Box<dyn HttpError>)->String {
    format!("<!DOCTYPE html>
<html>
<head>
    <meta charset=\"utf-8\">
    <title>{0}: {1}</title>
    </head>
    <body align=\"center\">
        <div align=\"center\">
            <h1>{0}: {1}</h1>
            <h3>{2}</h3>
            <p>{3}</p>
            <hr />
            <small>salvo</small>
        </div>
    </body>
</html>", e.code(), e.name(), e.summary(), e.detail())
}
fn error_json(e: &Box<dyn HttpError>)->String {
    format!("{{\"error\":{{\"code\":{},\"name\":\"{}\",\"summary\":\"{}\",\"detail\":\"{}\"}}}}",
        e.code(), e.name(), e.summary(), e.detail())
}
fn error_text(e: &Box<dyn HttpError>)->String {
   format!("code:{},\nname:{},\nsummary:{},\ndetail:{}", e.code(), e.name(), e.summary(), e.detail())
}
fn error_xml(e: &Box<dyn HttpError>)->String {
    format!("<error><code>{}</code><name>{}</name><summary>{}</summary><detail>{}</detail></error>", 
        e.code(), e.name(), e.summary(), e.detail())
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
        let dmime: Mime = "text/html".parse().unwrap();
        let accept = req.accept();
        let mut format = accept.first().unwrap_or(&dmime);
        if format.subtype() != mime::JSON && format.subtype() != mime::HTML {
            format = &dmime;
        }
        resp.headers_mut().insert(header::CONTENT_TYPE, format.to_string().parse().unwrap());
        let content = match format.subtype().as_ref(){
            "text"=> error_text(&self.0),
            "json"=> error_json(&self.0),
            "xml"=> error_xml(&self.0),
            _ => error_html(&self.0),
        };
        resp.body_writers.push(Box::new(content));
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

