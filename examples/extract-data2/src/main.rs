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
// #[handler]
// async fn edit<'a>(bad_man: LazyExtract<BadMan<'a>>, good_man: LazyExtract<GoodMan<'a>>, req: &mut Request) -> String {
//     let bad_man = bad_man.extract(req).await.unwrap();
//     let bad_man = format!("Bad Man: {:#?}", bad_man);
//     let good_man = good_man.extract(req).await.unwrap();
//     let good_man = format!("Good Man: {:#?}", good_man);
//     format!("{}\r\n\r\n\r\n{}", bad_man, good_man)
// }

#[allow(non_camel_case_types)]
#[derive(Debug)]
struct edit;
impl edit {
    async fn edit<'a>(
        bad_man: LazyExtract<BadMan<'a>>,
        good_man: LazyExtract<GoodMan<'a>>,
        req: &mut Request,
    ) -> String {
        {
            let bad_man = bad_man.extract(req).await.unwrap();
            let bad_man = format!("Bad Man: {:#?}", bad_man);
            let good_man = good_man.extract(req).await.unwrap();
            let good_man = format!("Good Man: {:#?}", good_man);
            format!("{}\r\n\r\n\r\n{}", bad_man, good_man)
        }
    }
}
#[salvo::async_trait]
impl salvo::Handler for edit {
    #[inline]
    async fn handle(
        &self,
        req: &mut salvo::Request,
        depot: &mut salvo::Depot,
        res: &mut salvo::Response,
        ctrl: &mut salvo::routing::FlowCtrl,
    ) {
        let bad_man: LazyExtract<BadMan> = match req.extract().await {
            Ok(data) => data,
            Err(e) => {
                salvo :: __private :: tracing :: error!
                (error = ? e, "failed to extract data");
                res.set_status_error(
                    salvo::http::errors::StatusError::bad_request().with_detail("Extract data failed."),
                );
                return;
            }
        };
        let good_man: LazyExtract<GoodMan> = match req.extract().await {
            Ok(data) => data,
            Err(e) => {
                salvo :: __private :: tracing :: error!
                (error = ? e, "failed to extract data");
                res.set_status_error(
                    salvo::http::errors::StatusError::bad_request().with_detail("Extract data failed."),
                );
                return;
            }
        };
        salvo::Writer::write(Self::edit(bad_man, good_man, req).await, req, depot, res).await;
    }
}

#[derive(Serialize, Deserialize, Extractible, Debug)]
#[extract(
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
#[derive(Serialize, Deserialize, Extractible, Debug)]
#[extract(
    default_source(from = "query"),
    default_source(from = "param"),
    default_source(from = "body")
)]
struct GoodMan<'a> {
    id: i64,
    username: &'a str,
    first_name: String,
    last_name: String,
    #[extract(alias = "lovers")]
    lover: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<id>").get(show).post(edit);
    tracing::info!("Listening on http://127.0.0.1:7878");
    println!("Example url: http://127.0.0.1:7878/95");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
