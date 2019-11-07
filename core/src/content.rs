use crate::Context;

pub trait Content {
    fn apply(self, ctx: &mut Context);
}

pub struct HtmlContent<T>(T) where T: Into<String>;
impl<T> Content for HtmlContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("text/html", self.0.into());
    }
}

pub struct JsonContent<T>(T) where T: Into<String>;
impl<T> Content for JsonContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("application/json", self.0.into());
    }
}

pub struct TextContent<T>(T) where T: Into<String>;
impl<T> Content for TextContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("text/plain", self.0.into());
    }
}

pub struct XmlContent<T>(T) where T: Into<String>;
impl<T> Content for XmlContent<T> where T: Into<String> {
    fn apply(self, ctx: &mut Context){
        ctx.render("text/xml", self.0.into());
    }
}