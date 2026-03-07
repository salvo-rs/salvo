use salvo::oapi::extract::*;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, ToSchema, Debug)]
#[salvo(schema(aliases(MyI32 = MyObject<i32>, MyStr = MyObject<String>)))]
struct MyObject<T: ToSchema + std::fmt::Debug + 'static> {
    value: T,
}

/// A DTO with custom schema name.
/// When used in generics like `Response<CityDTO>`, the schema name will be `Response<City>`
/// instead of `Response<full::path::to::CityDTO>`.
#[derive(Serialize, Deserialize, ToSchema, Debug)]
#[salvo(schema(name = City))]
struct CityDTO {
    id: u64,
    name: String,
}

/// Generic response wrapper with custom name.
/// When instantiated as `Response<CityDTO>`, it becomes `Response<City>` in the schema.
/// When instantiated as `Response<String>`, it becomes `Response<String>` (primitive types are shortened).
#[derive(Serialize, Deserialize, ToSchema, Debug)]
#[salvo(schema(name = Response))]
struct ApiResponse<T: ToSchema + std::fmt::Debug + 'static> {
    code: u32,
    data: T,
}

/// Use string type, this will add to openapi doc.
#[endpoint]
async fn use_string(body: JsonBody<MyObject<String>>) -> String {
    format!("{body:?}")
}

/// Use i32 type, this will add to openapi doc.
#[endpoint]
async fn use_i32(body: JsonBody<MyObject<i32>>) -> String {
    format!("{body:?}")
}

/// Use u64 type, this will add to openapi doc.
#[endpoint]
async fn use_u64(body: JsonBody<MyObject<u64>>) -> String {
    format!("{body:?}")
}

/// Returns Response<City> - demonstrates custom name resolution in generics.
#[endpoint]
async fn get_city() -> Json<ApiResponse<CityDTO>> {
    Json(ApiResponse {
        code: 200,
        data: CityDTO {
            id: 1,
            name: "Beijing".to_string(),
        },
    })
}

/// Returns Response<String> - demonstrates primitive type shortening in generics.
#[endpoint]
async fn get_message() -> Json<ApiResponse<String>> {
    Json(ApiResponse {
        code: 200,
        data: "Hello, World!".to_string(),
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // Custom your OpenApi naming style. You should set it before using OpenApi.
    salvo::oapi::naming::set_namer(
        salvo::oapi::naming::FlexNamer::new()
            .short_mode(true)
            .generic_delimiter('_', '_'),
    );

    let router = Router::new()
        .push(Router::with_path("i32").post(use_i32))
        .push(Router::with_path("u64").post(use_u64))
        .push(Router::with_path("string").post(use_string))
        .push(Router::with_path("city").get(get_city))
        .push(Router::with_path("message").get(get_message));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("/swagger-ui"));

    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}
