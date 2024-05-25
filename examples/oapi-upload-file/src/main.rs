use std::fs::create_dir_all;
use std::path::Path;

use salvo::oapi::extract::*;
use salvo::prelude::*;

#[handler]
async fn index(res: &mut Response) {
    res.render(Text::Html(INDEX_HTML));
}
#[endpoint]
async fn upload(file: FormFile, res: &mut Response) {
    let dest = format!("temp/{}", file.name().unwrap_or("file"));
    println!("{dest}");
    let info = if let Err(e) = std::fs::copy(file.path(), Path::new(&dest)) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        format!("file not found in request: {e}")
    } else {
        format!("File uploaded to {dest}")
    };
    res.render(Text::Plain(info));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    create_dir_all("temp").unwrap();
    let router = Router::new().get(index).post(upload);

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("/swagger-ui"));

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
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
