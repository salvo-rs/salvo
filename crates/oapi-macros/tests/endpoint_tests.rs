#![allow(missing_docs)]
use assert_json_diff::assert_json_eq;
use salvo::oapi::extract::*;
use salvo::prelude::*;
use serde_json::json;

#[test]
fn test_endpoint_hello() {
    #[endpoint]
    async fn hello(name: QueryParam<String, false>) -> String {
        format!("Hello, {}!", name.as_deref().unwrap_or("World"))
    }

    let router = Router::new().push(Router::with_path("hello").get(hello));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);
    assert_json_eq!(
        doc,
        json!({
            "openapi":"3.1.0",
            "info":{
                "title":"test api",
                "version":"0.0.1"
            },
            "paths":{
                "/hello":{
                    "get":{
                        "operationId":"endpoint_tests.test_endpoint_hello.hello",
                        "parameters":[{
                                "name":"name",
                                "in":"query",
                                "description":"Get parameter `name` from request url query.",
                                "required":false,"schema":{"type":"string"}
                            }],
                        "responses":{
                            "200":{
                                "description":"Ok",
                                "content":{"text/plain":{"schema":{"type":"string"}}}
                            }
                        }
                    }
                }
            }
        })
    );
}

#[test]
fn test_endpoint_singular_aliases() {
    #[endpoint(
        tag("pets"),
        parameter("id" = String, Path, description = "Pet id"),
        response(status_code = 404, description = "Not found")
    )]
    async fn show_pet() -> String {
        "pet".to_owned()
    }

    let router = Router::new().push(Router::with_path("pets/{id}").get(show_pet));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);
    assert_json_eq!(
        doc,
        json!({
            "openapi":"3.1.0",
            "info":{
                "title":"test api",
                "version":"0.0.1"
            },
            "paths":{
                "/pets/{id}":{
                    "get":{
                        "operationId":"endpoint_tests.test_endpoint_singular_aliases.show_pet",
                        "tags":["pets"],
                        "parameters":[{
                            "name":"id",
                            "in":"path",
                            "description":"Pet id",
                            "required":true,
                            "schema":{"type":"string"}
                        }],
                        "responses":{
                            "200":{
                                "description":"Ok",
                                "content":{"text/plain":{"schema":{"type":"string"}}}
                            },
                            "404":{
                                "description":"Not found"
                            }
                        }
                    }
                }
            }
        })
    );
}

// The `request_body(...)` attribute must expand to paths that resolve through the `salvo`
// facade, which is how this test crate (and most applications) reach `salvo-oapi`.
#[test]
fn test_endpoint_request_body_attribute() {
    #[derive(serde::Deserialize, ToSchema)]
    #[allow(dead_code)]
    struct NewUser {
        name: String,
    }

    #[endpoint(request_body(content = NewUser, description = "user payload"))]
    async fn create_user() -> &'static str {
        "ok"
    }

    // The attribute is merged into the body inferred from the `JsonBody` extractor.
    #[endpoint(request_body(content = NewUser, description = "replacement payload"))]
    async fn replace_user(_user: JsonBody<NewUser>) -> &'static str {
        "ok"
    }

    #[endpoint(request_body = NewUser)]
    async fn patch_user() -> &'static str {
        "ok"
    }

    let router = Router::new()
        .push(Router::with_path("users").post(create_user))
        .push(
            Router::with_path("users/{id}")
                .put(replace_user)
                .patch(patch_user),
        );

    let doc =
        serde_json::to_value(OpenApi::new("test api", "0.0.1").merge_router(&router)).unwrap();
    let schema_ref = json!({"$ref": "#/components/schemas/endpoint_tests.test_endpoint_request_body_attribute.NewUser"});
    assert_json_eq!(
        doc["paths"]["/users"]["post"]["requestBody"],
        json!({
            "description": "user payload",
            "content": {"application/json": {"schema": schema_ref}},
            "required": true
        })
    );
    let merged = &doc["paths"]["/users/{id}"]["put"]["requestBody"];
    assert_eq!(merged["description"], "replacement payload");
    assert_eq!(merged["content"]["application/json"]["schema"], schema_ref);
    let short_form = &doc["paths"]["/users/{id}"]["patch"]["requestBody"];
    assert_eq!(
        short_form["content"]["application/json"]["schema"],
        schema_ref
    );
}
