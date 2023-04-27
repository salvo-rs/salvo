use salvo::oapi::extract::*;
use salvo::oapi::swagger::SwaggerUi;
use salvo::oapi::{AsSchema, Info, OpenApi};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, AsSchema, Debug)]
struct MyObject<T: AsSchema + std::fmt::Debug> {
    value: T,
}

#[endpoint]
async fn string(body: JsonBody<MyObject<String>>, res: &mut Response) {
    res.render(format!("{:?}", body))
}
// #[endpoint]
// async fn i32(body: JsonBody<MyObject<i32>>, res: &mut Response) {
//     res.render(format!("{:?}", body))
// }

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        // .push(Router::with_path("i32").get(i32))
        .push(Router::with_path("string").post(string));

    let doc = OpenApi::new(Info::new("test api", "0.0.1")).merge_router(&router);

    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("swagger-ui"));
    println!("{:?}", <MyObject<String> as AsSchema>::symbol());

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
