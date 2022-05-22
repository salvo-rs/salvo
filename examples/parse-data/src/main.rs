use enumflags2::make_bitflags;
use salvo::http::ParseSource;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[fn_handler]
async fn show(req: &mut Request, res: &mut Response) {
    let content = format!(
        r#"<!DOCTYPE html>
    <html>
        <head>
            <title>Parse data</title>
        </head>
        <body>
            <h1>Hello, fill your profile</h1>
            <form action="/{}" method="post">
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
#[fn_handler]
async fn edit(req: &mut Request) -> String {
    let source = make_bitflags!(ParseSource::{Params|Queries|Form});
    let bad_man: BadMan = req.parse_data(source).await.unwrap();
    let bad_man = format!("Bad Man: {:#?}", bad_man);
    let good_man: GoodMan = req.parse_data(source).await.unwrap();
    let good_man = format!("Good Man: {:#?}", good_man);
    format!("{}\r\n\r\n\r\n{}", bad_man, good_man)
}

#[derive(Debug, Serialize, Deserialize)]
struct BadMan<'a> {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    username: &'a str,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: &'a str,
    #[serde(default)]
    lovers: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct GoodMan<'a> {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    username: &'a str,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: &'a str,
    #[serde(default)]
    #[serde(alias = "lovers")]
    lover: &'a str,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<id>").get(show).post(edit);
    tracing::info!("Listening on http://127.0.0.1:7878");
    println!("Example url: http://127.0.0.1:7878/95?username=jobs");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
