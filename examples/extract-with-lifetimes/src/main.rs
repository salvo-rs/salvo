use salvo::macros::Extractible;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[handler]
async fn show(req: &mut Request, res: &mut Response) {
    let content = format!(
        r#"<!DOCTYPE html>
    <html>
        <head>
            <title>Parse data</title>
        </head>
        <body>
            <h1>Hello, fill your profile</h1>
            <form action="/{}?username=jobs" method="post">
                <label>First Name:</label><input type="text" name="first_name" />
                <label>Last Name:</label><input type="text" name="last_name" />
                <legend>What is Your Favorite Pet?</legend>
                <input type="checkbox" name="lovers" value="Cats">Cats<br>
                <input type="checkbox" name="lovers" value="Dogs">Dogs<br>
                <input type="checkbox" name="lovers" value="Birds">Birds<br>
                <input type="submit" value="Submit" />
            </form>
        </body>
    </html>
    "#,
        req.params().get("id").unwrap()
    );
    res.render(Text::Html(content));
}

#[handler]
async fn edit<'a>(bad_man: BadMan<'a>) -> String {
    format!("{bad_man:#?}")
}

#[derive(Serialize, Deserialize, Extractible, Debug)]
#[salvo(
    default_source(from = "query"),
    default_source(from = "param"),
    default_source(from = "body")
)]
struct BadMan<'a> {
    id: i64,
    username: String,
    first_name: &'a str,
    last_name: String,
    lovers: Vec<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("{id}").get(show).post(edit);

    println!("Example url: http://0.0.0.0:5800/95");
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
