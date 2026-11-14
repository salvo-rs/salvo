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

// `links(...)` must expand to a path that exists: `Link` is re-exported from the crate
// root, its `link` module is private.
#[test]
fn test_endpoint_response_links() {
    #[endpoint(responses(
        (status_code = 200, description = "success response",
            links(
                ("link_name" = (
                    operation_id = "test_links",
                    parameters(("key" = "value"), ("json_value" = json!(1))),
                    request_body = "this is body",
                    description = "follow up",
                    server(url = "http://localhost")
                )),
                ("by_ref" = (operation_ref = "#/paths/~1users/get"))
            )
        )
    ))]
    async fn linked() {}

    let router = Router::new().push(Router::with_path("linked").get(linked));
    let doc =
        serde_json::to_value(OpenApi::new("test api", "0.0.1").merge_router(&router)).unwrap();
    assert_json_eq!(
        doc["paths"]["/linked"]["get"]["responses"]["200"]["links"],
        json!({
            "link_name": {
                "operationId": "test_links",
                "parameters": {"key": "value", "json_value": 1},
                "requestBody": "this is body",
                "description": "follow up",
                "server": {"url": "http://localhost"}
            },
            "by_ref": {"operationRef": "#/paths/~1users/get"}
        })
    );
}

// `allow_reserved` takes a `bool` on `Parameter`; both the tuple form and the
// `ToParameters` field attribute must compile and reach the document.
#[test]
fn test_endpoint_parameter_allow_reserved() {
    #[derive(serde::Deserialize, ToParameters)]
    #[salvo(parameters(default_parameter_in = Query))]
    #[allow(dead_code)]
    struct Search {
        #[salvo(parameter(allow_reserved))]
        filter: String,
        #[salvo(parameter(allow_reserved = false))]
        strict: String,
        plain: String,
    }

    #[endpoint(parameters(
        ("q" = String, Query, allow_reserved),
        ("r" = String, Query, allow_reserved = false),
    ))]
    async fn tuple_form() {}

    #[endpoint]
    async fn derive_form(_search: Search) {}

    let router = Router::new()
        .push(Router::with_path("tuple").get(tuple_form))
        .push(Router::with_path("derive").get(derive_form));
    let doc =
        serde_json::to_value(OpenApi::new("test api", "0.0.1").merge_router(&router)).unwrap();

    let tuple = &doc["paths"]["/tuple"]["get"]["parameters"];
    assert_eq!(tuple[0]["name"], "q");
    assert_eq!(tuple[0]["allowReserved"], true);
    assert_eq!(tuple[1]["name"], "r");
    assert_eq!(tuple[1]["allowReserved"], false);

    let derive = &doc["paths"]["/derive"]["get"]["parameters"];
    assert_eq!(derive[0]["name"], "filter");
    assert_eq!(derive[0]["allowReserved"], true);
    assert_eq!(derive[1]["name"], "strict");
    assert_eq!(derive[1]["allowReserved"], false);
    assert_eq!(derive[2]["name"], "plain");
    assert!(derive[2].get("allowReserved").is_none());
}
