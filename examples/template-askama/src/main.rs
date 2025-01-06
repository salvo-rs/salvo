use askama::Template;
use salvo::prelude::*;

#[derive(Template)]
#[template(path = "hello.html")]
struct HelloTemplate<'a> {
    name: &'a str,
}

#[handler]
async fn hello(req: &mut Request, res: &mut Response) {
    let hello_tmpl = HelloTemplate {
        name: req.param::<&str>("name").unwrap_or("World"),
    };
    res.render(Text::Html(hello_tmpl.render().unwrap()));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("{name}").get(hello);
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
