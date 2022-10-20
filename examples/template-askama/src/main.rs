use askama::Template;
use salvo::prelude::*;

#[derive(Template)]
#[template(path = "hello.html")]
struct HelloTemplate<'a> {
    name: &'a str,
}

#[handler]
async fn hello(req: &mut Request, res: &mut Response) {
    let hello = HelloTemplate {
        name: req.param::<&str>("name").unwrap_or("World"),
    };
    res.render(Text::Html(hello.render().unwrap()));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<name>").get(hello);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
