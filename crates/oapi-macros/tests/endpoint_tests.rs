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
