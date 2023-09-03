use salvo_core::prelude::*;
use salvo_core::Router;
use salvo_oapi::extract::*;
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
            (status_code = 200, description = "Pet found successfully"),
            (status_code = 404, description = "Pet was not found")
        ),
        parameters(
            ("id", description = "Pet database id to get Pet for"),
        ),
        security(
            (),
            ("my_auth" = ["read:items", "edit:items"]),
            ("token_jwt" = [])
        )
    )]
    #[allow(unused)]
    pub async fn get_pet_by_id(pet_id: PathParam<u64>) -> Json<Pet> {
        let pet = Pet {
            id: pet_id.into_inner(),
            age: None,
            name: "lightning".to_string(),
        };
        Json(pet)
    }
}

#[test]
fn oapi_test() {
    let mut doc = salvo_oapi::OpenApi::new("my application", "0.1.0").add_server(
        Server::new("/api/bar/{username}")
            .description("this is description of the server")
            .add_variable(
                "username",
                ServerVariable::new()
                    .default_value("the_user")
                    .description("this is user"),
            ),
    );
    doc.components.security_schemes.insert(
        "token_jwt".into(),
        SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer).bearer_format("JWT")),
    );

    let router = Router::with_path("/pets/{id}").get(pet_api::get_pet_by_id);

    println!("{}", doc.to_pretty_json().unwrap());
}
