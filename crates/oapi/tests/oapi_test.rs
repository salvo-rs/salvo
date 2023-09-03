#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use salvo_oapi::{
    openapi::{
        self,
        security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
        server::{ServerBuilder, ServerVariableBuilder},
    },
    Modify, OpenApi, ToSchema,
};

#[derive(Deserialize, Serialize, ToSchema)]
#[schema(example = json!({"name": "bob the cat", "id": 1}))]
struct Pet {
    id: u64,
    name: String,
    age: Option<i32>,
}

mod pet_api {
    use super::*;

    const ID: &str = "get_pet";

    /// Get pet by id
    ///
    /// Get pet from database by pet database id
    #[salvo_oapi::endpoint(
        get,
        operation_id = ID,
        path = "/pets/{id}",
        responses(
            (status = 200, description = "Pet found successfully", body = Pet),
            (status = 404, description = "Pet was not found")
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
    async fn get_pet_by_id(pet_id: u64) -> Pet {
        Pet {
            id: pet_id,
            age: None,
            name: "lightning".to_string(),
        }
    }
}

#[derive(Default, OpenApi)]
#[openapi(
    paths(pet_api::get_pet_by_id),
    components(schemas(Pet, GenericC, GenericD)),
    modifiers(&Foo),
    security(
        (),
        ("my_auth" = ["read:items", "edit:items"]),
        ("token_jwt" = [])
    )
)]
struct ApiDoc;

macro_rules! build_foo {
    ($typ: ident, $d: ty, $r: ty) => {
        #[derive(Debug, Serialize, ToSchema)]
        struct $typ {
            data: $d,
            resources: $r,
        }
    };
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
#[aliases(GenericC = C<A, B>, GenericD = C<B, A>)]
struct C<T, R> {
    field_1: R,
    field_2: T,
}

impl Modify for Foo {
    fn modify(&self, openapi: &mut openapi::OpenApi) {
        if let Some(schema) = openapi.components.as_mut() {
            schema.add_security_scheme(
                "token_jwt",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            )
        }

        openapi.servers = Some(vec![ServerBuilder::new()
            .url("/api/bar/{username}")
            .description(Some("this is description of the server"))
            .parameter(
                "username",
                ServerVariableBuilder::new()
                    .default_value("the_user")
                    .description(Some("this is user")),
            )
            .build()]);
    }
}

#[derive(Debug, Serialize)]
struct Foo;

#[derive(Debug, Serialize)]
struct FooResources;

#[test]
#[ignore = "this is just a test bed to run macros"]
fn derive_openapi() {
    salvo_oapi::openapi::OpenApi::new(
        salvo_oapi::openapi::Info::new("my application", "0.1.0"),
        salvo_oapi::openapi::Paths::new(),
    );
    println!("{}", ApiDoc::openapi().to_pretty_json().unwrap());

    build_foo!(GetFooBody, Foo, FooResources);
}
