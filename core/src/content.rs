use crate::Context;

pub trait Content {
    fn apply(&self, ctx: &mut Context);
}

impl Content for Box<dyn Content> {
    fn apply(&self, ctx: &mut Context) {
        (**self).apply(ctx);
    }
}

pub struct HtmlTextContent(String);
impl Content for HtmlTextContent {
    fn apply(&self, ctx: &mut Context){
        ctx.render("text/html", self.0.clone());
    }
}

pub struct JsonTextContent(String);
impl Content for JsonTextContent {
    fn apply(&self, ctx: &mut Context){
        ctx.render("application/json", self.0.clone());
    }
}

pub struct PlainTextContent(String);
impl Content for PlainTextContent {
    fn apply(&self, ctx: &mut Context){
        ctx.render("text/plain", self.0.clone());
    }
}

pub struct XmlTextContent(String);
impl Content for XmlTextContent{
    fn apply(&self, ctx: &mut Context){
        ctx.render("text/xml", self.0.clone());
    }
}