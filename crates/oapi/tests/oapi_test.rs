use salvo_core::Router;
use salvo_oapi::{
    security::{Http, HttpAuthScheme, SecurityScheme},
    server::{Server, ServerVariable},
    OpenApi, ToSchema,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, ToSchema)]
#[salvo(schema(example = json!({"name": "bob the cat", "id": 1})))]
struct Pet {
    id: u64,
    name: String,
    age: Option<i32>,
}

mod pet_api {
    use super::*;

    /// Get pet by id
    ///
    /// Get pet from database by pet database id
    #[salvo_oapi::endpoint(
        responses(
            (status_code = 200, description = "Pet found successfully", body = Pet),
            (status_code = 404, description = "Pet was not found")
        ),
        parameters(
            ("id" = u64, Path, description = "Pet database id to get Pet for"),
        ),
        security(
            (),
            ("my_auth" = ["read:items", "edit:items"]),
            ("token_jwt" = [])
        )
    )]
    #[allow(unused)]
    pub async fn get_pet_by_id(pet_id: u64) -> Pet {
        Pet {
            id: pet_id,
            age: None,
            name: "lightning".to_string(),
        }
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
struct A {
    a: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
struct B {
    b: i64,
}

#[derive(Deserialize, Serialize, ToSchema)]
struct C<T, R> {
    field_1: R,
    field_2: T,
}

#[derive(Debug, Serialize)]
struct Foo;

#[derive(Debug, Serialize)]
struct FooResources;

#[test]
#[ignore = "this is just a test bed to run macros"]
fn oapi_test() {
    let doc = salvo_oapi::OpenApi::new("my application", "0.1.0");
    doc.components.security_scheme(
        "token_jwt",
        SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer).bearer_format("JWT")),
    );

    doc.servers = Some(vec![Server::new("/api/bar/{username}")
        .description("this is description of the server")
        .parameter(
            "username",
            ServerVariable::new()
                .default_value("the_user")
                .description("this is user"),
        )]);

    //
    // security(
    //     (),
    //     ("my_auth" = ["read:items", "edit:items"]),
    //     ("token_jwt" = [])
    // )
    let router = Router::with_path("/pets/{id}").get(pet_api::get_pet_by_id);

    println!("{}", ApiDoc::openapi().to_pretty_json().unwrap());
}
