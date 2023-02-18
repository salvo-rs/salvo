use std::fs::create_dir_all;
use std::path::Path;

use salvo::prelude::*;

#[handler]
async fn index(res: &mut Response) {
    res.render(Text::Html(INDEX_HTML));
}
#[handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let file = req.file("file").await;
    if let Some(file) = file {
        let dest = format!("temp/{}", file.name().unwrap_or("file"));
        println!("{dest}");
        let info = if let Err(e) = std::fs::copy(file.path(), Path::new(&dest)) {
            res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            format!("file not found in request: {e}")
        } else {
            format!("File uploaded to {dest}")
        };
        res.render(Text::Plain(info));
    } else {
        res.set_status_code(StatusCode::BAD_REQUEST);
        res.render(Text::Plain("file not found in request"));
    };
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    create_dir_all("temp").unwrap();
    let router = Router::new().get(index).post(upload);

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Upload file</title>
    </head>
    <body>
        <h1>Upload file</h1>
        <form action="/" method="post" enctype="multipart/form-data">
            <input type="file" name="file" />
            <input type="submit" value="upload" />
        </form>
    </body>
</html>
"#;
