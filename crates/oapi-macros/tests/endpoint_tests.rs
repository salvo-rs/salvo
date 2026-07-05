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
