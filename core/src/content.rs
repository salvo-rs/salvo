use crate::Context;

pub trait Content {
    fn apply(self, ctx: &mut Context);
}

pub struct HtmlTextContent<T>(T) where T: Into<String>;
impl<T> Content for HtmlTextContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("text/html", self.0.into());
    }
}

pub struct JsonTextContent<T>(T) where T: Into<String>;
impl<T> Content for JsonTextContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("application/json", self.0.into());
    }
}

pub struct PlainTextContent<T>(T) where T: Into<String>;
impl<T> Content for PlainTextContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("text/plain", self.0.into());
    }
}

pub struct XmlTextContent<T>(T) where T: Into<String>;
impl<T> Content for XmlTextContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("text/xml", self.0.into());
    }
}