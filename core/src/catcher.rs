use http::status::StatusCode;
use mime::Mime;
use crate::http::{headers, Response, Request};

pub trait Catcher: Send + Sync + 'static {
    fn catch(&self, req: &Request, resp: &mut Response)->bool;
}

fn error_html(code: u16, name: &str, summary: &str)->String {
    format!("<!DOCTYPE html>
<html>
<head>
    <meta charset=\"utf-8\">
    <title>{0}: {1}</title>
    </head>
    <body align=\"center\">
        <div align=\"center\">
            <h1>{0}: {1}</h1>
            <p>{2}</p>
            <hr />
            <small>novel</small>
        </div>
    </body>
</html>", code, name, summary)
}
fn error_json(code: u16, name: &str, summary: &str)->String {
   format!("{{\"code\":{},\"name\":\"{}\",\"summary\":\"{}\"}}", code, name, summary)
}
fn error_text(code: u16, name: &str, summary: &str)->String {
   format!("code:{},\nname:{},\nsummary:{}", code, name, summary)
}
fn error_xml(code: u16, name: &str, summary: &str)->String {
    format!("<error><code>{}</code><name>{}</name><summary>{}</summary></error>", code, name, summary)
}
pub struct CatcherImpl{
    code: u16,
    name: String,
    summary: String,
}
impl CatcherImpl{
    pub fn new(code: u16, name: String, summary: String)->CatcherImpl{
        CatcherImpl{
            code,
            name,
            summary,
        }
    }
}
impl Catcher for CatcherImpl {
    fn catch(&self, req: &Request, resp: &mut Response)->bool {
        let status = resp.status.unwrap_or(StatusCode::NOT_FOUND);
        if status.as_u16() != self.code {
            return false;
        }
        let dmime: Mime = "text/html".parse().unwrap();
        let accept = req.accept();
        let mut format = accept.first().unwrap_or(&dmime);
        if format.type_() != "text" {
            format = &dmime;
        }
        resp.headers.insert(headers::CONTENT_TYPE, format.to_string().parse().unwrap());
        let content = match format.subtype().as_ref(){
            "text"=> error_text(self.code, &self.name, &self.summary),
            "json"=> error_json(self.code, &self.name, &self.summary),
            "xml"=> error_xml(self.code, &self.name, &self.summary),
            _ => error_html(self.code, &self.name, &self.summary),
        };
        resp.body_writers.push(Box::new(content));
        true
    }
}

macro_rules! default_catchers {
    ($($code:expr, $name:expr, $summary:expr),+) => (
        let mut list: Vec<Box<dyn Catcher>> = vec![];
        $(
            list.push(Box::new(CatcherImpl::new($code, $name.to_owned(), $summary.to_owned())));
        )+
        list
    )
}

pub mod defaults {
    use super::{Catcher, CatcherImpl};

    pub fn get() -> Vec<Box<dyn Catcher>> {
        default_catchers! {
            400, "Bad Request", "The request could not be understood by the server due to malformed syntax.", 
            401, "Unauthorized", "The request requires user authentication.",
            402, "Payment Required", "The request could not be processed due to lack of payment.",
            403, "Forbidden", "The server refused to authorize the request.",
            404, "Not Found", "The requested resource could not be found.",
            405, "Method Not Allowed", "The request method is not supported for the requested resource.",
            406, "Not Acceptable", "The requested resource is capable of generating only content not acceptable 
                according to the Accept headers sent in the request.",
            407, "Proxy Authentication Required", "Authentication with the proxy is required.", 
            408, "Request Timeout", "The server timed out waiting for the request.",
            409, "Conflict", "The request could not be processed because of a conflict in the request.",
            410, "Gone", "The resource requested is no longer available and will not be available again.",
            411, "Length Required", "The request did not specify the length of its content, which is required by the requested resource.",
            412, "Precondition Failed", "The server does not meet one of the preconditions specified in the request.",
            413, "Payload Too Large", "The request is larger than the server is willing or able to process.",
            414, "URI Too Long", "The URI provided was too long for the server to process.",
            415, "Unsupported Media Type", "The request entity has a media type which the server or resource does not support.",
            416, "Range Not Satisfiable", "The portion of the requested file cannot be supplied by the server.",
            417, "Expectation Failed", "The server cannot meet the requirements of the expect request-header field.",
            418, "I'm a teapot", "I was requested to brew coffee, and I am a teapot.",
            421, "Misdirected Request", "The server cannot produce a response for this request.",
            422, "Unprocessable Entity", "The request was well-formed but was unable to be followed due to semantic errors.",
            426, "Upgrade Required", "Switching to the protocol in the Upgrade header field is required.",
            428, "Precondition Required", "The server requires the request to be conditional.",
            429, "Too Many Requests", "Too many requests have been received recently.",
            431, "Request Header Fields Too Large", "The server is unwilling to process the request because either 
                an individual header field, or all the header fields collectively, are too large.",
            451, "Unavailable For Legal Reasons", "The requested resource is unavailable due to a legal demand to deny access to this resource.", 
            500, "Internal Server Error", "The server encountered an internal error while processing this request.",
            501, "Not Implemented", "The server either does not recognize the request method, or it lacks the ability to fulfill the request.",
            503, "Service Unavailable", "The server is currently unavailable.",
            504, "Gateway Timeout", "The server did not receive a timely response from an upstream server.",
            510, "Not Extended", "Further extensions to the request are required for the server to fulfill it."
        }
    }
}

