use crate::http::{Response};

pub trait Content {
    fn apply(self, resp: &mut Response);
}

pub struct HtmlTextContent<T>(T) where T: Into<String>;
impl<T> Content for HtmlTextContent<T> where T: Into<String> {
    fn apply(self, resp: &mut Response) {
        resp.render("text/html", self.0.into());
    }
}

pub struct JsonTextContent<T>(T) where T: Into<String>;
impl<T> Content for JsonTextContent<T> where T: Into<String> {
    fn apply(self, resp: &mut Response) {
        resp.render("application/json", self.0.into());
    }
}

pub struct PlainTextContent<T>(T) where T: Into<String>;
impl<T> Content for PlainTextContent<T> where T: Into<String> {
    fn apply(self, resp: &mut Response) {
        resp.render("text/plain", self.0.into());
    }
}

pub struct XmlTextContent<T>(T) where T: Into<String>;
impl<T> Content for XmlTextContent<T> where T: Into<String> {
    fn apply(self, resp: &mut Response) {
        resp.render("text/xml", self.0.into());
    }
}