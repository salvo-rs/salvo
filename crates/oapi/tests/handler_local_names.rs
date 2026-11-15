//! Function-like handlers must allow equally named local bindings.
#![deny(non_local_definitions, unused_variables)]

use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Router, Service};
use salvo_oapi::OpenApi;

mod handler_case {
    #[salvo_core::handler]
    pub(super) async fn list() -> &'static str {
        let list = "xxx";
        println!("==== {list}");
        "test"
    }
}

mod endpoint_case {
    #[salvo_oapi::endpoint]
    pub(super) async fn list() -> &'static str {
        let list = "xxx";
        println!("==== {list}");
        "test"
    }
}

#[tokio::test]
async fn zero_argument_handlers_allow_locals_named_after_the_function() {
    let router = Router::new()
        .push(Router::with_path("handler").get(handler_case::list))
        .push(Router::with_path("endpoint").get(endpoint_case::list));
    let document = serde_json::to_value(OpenApi::new("local names", "1").merge_router(&router))
        .expect("serialize OpenAPI document");
    assert!(document["paths"]["/endpoint"]["get"].is_object());
    assert!(
        document["paths"]["/endpoint"]["get"]["operationId"]
            .as_str()
            .expect("endpoint operation ID")
            .ends_with("endpoint_case.list")
    );
    let service = Service::new(router);
    for path in ["handler", "endpoint"] {
        let body = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await
            .take_string()
            .await
            .expect("handler response body");
        assert_eq!(body, "test");
    }
}

macro_rules! binding_cases {
    ($attribute:path) => {
        #[$attribute]
        fn tuple() -> String {
            let (tuple, _) = ("tuple", 0);
            let _: Self = Self;
            let _: Self = self::tuple;
            assert!(matches!(Self, Self));
            format!("{tuple}")
        }

        #[$attribute]
        async fn conditional() -> String {
            if let Some(conditional) = Some("conditional") {
                format!("{conditional}")
            } else {
                unreachable!()
            }
        }

        #[$attribute]
        fn matching() -> String {
            match Some("matching") {
                Some(matching) => matching.to_owned(),
                None => unreachable!(),
            }
        }

        #[$attribute]
        fn iteration() -> String {
            let mut result = String::new();
            for iteration in ["iteration"] {
                result.push_str(iteration);
            }
            result
        }

        #[$attribute]
        fn closure() -> String {
            let invoke = |closure: &str| closure.to_owned();
            invoke("closure")
        }

        #[$attribute]
        fn r#raw_local() -> String {
            let raw_local = "raw";
            format!("{raw_local}")
        }

        #[$attribute]
        fn nested_module() -> String {
            mod child {
                pub(super) fn value() -> &'static str {
                    let nested_module = "module";
                    nested_module
                }
            }
            let _: Self = nested_module;
            child::value().to_owned()
        }

        #[$attribute]
        fn shadowed_item() -> String {
            let _: Self = shadowed_item;
            {
                fn shadowed_item() {}
                shadowed_item();
                let shadowed_item = "item";
                shadowed_item.to_owned()
            }
        }

        mod support {
            pub(super) fn source() {}
            pub(super) fn wildcard_import() {}
        }

        #[$attribute]
        fn r#shadowed_import() -> String {
            let _: Self = r#shadowed_import;
            {
                use self::support::source as shadowed_import;
                shadowed_import();
                let shadowed_import = "import";
                shadowed_import.to_owned()
            }
        }

        #[$attribute]
        fn wildcard_import() -> String {
            let _: Self = wildcard_import;
            {
                use self::support::*;
                wildcard_import();
                let wildcard_import = "wildcard";
                wildcard_import.to_owned()
            }
        }

        #[$attribute]
        fn unused_expansion() -> String {
            #[allow(unused_macros)]
            macro_rules! unused {
                () => {
                    let unused_expansion = "unused";
                };
            }
            let _: Self = unused_expansion;
            "unexpanded".to_owned()
        }

        #[$attribute]
        fn trait_declaration() -> String {
            #[allow(dead_code)]
            trait Declaration {
                fn accept(trait_declaration: u8);
            }
            let _: Self = trait_declaration;
            "trait declaration".to_owned()
        }

        pub(super) fn router() -> Router {
            Router::new()
                .push(Router::with_path("tuple").get(tuple))
                .push(Router::with_path("conditional").get(conditional))
                .push(Router::with_path("matching").get(matching))
                .push(Router::with_path("iteration").get(iteration))
                .push(Router::with_path("closure").get(closure))
                .push(Router::with_path("raw").get(r#raw_local))
                .push(Router::with_path("module").get(nested_module))
                .push(Router::with_path("item").get(shadowed_item))
                .push(Router::with_path("import").get(r#shadowed_import))
                .push(Router::with_path("wildcard").get(wildcard_import))
                .push(Router::with_path("unexpanded").get(unused_expansion))
                .push(Router::with_path("trait-declaration").get(trait_declaration))
        }
    };
}

mod handlers {
    use super::Router;
    binding_cases!(salvo_core::handler);
}

mod endpoints {
    use super::Router;
    binding_cases!(salvo_oapi::endpoint);
}

#[tokio::test]
async fn parsed_binding_patterns_work_without_changing_independent_scopes() {
    let router = Router::new()
        .push(Router::with_path("handlers").push(handlers::router()))
        .push(Router::with_path("endpoints").push(endpoints::router()));
    let document = serde_json::to_value(OpenApi::new("binding scopes", "1").merge_router(&router))
        .expect("serialize OpenAPI document");
    let service = Service::new(router);
    for prefix in ["handlers", "endpoints"] {
        for (path, expected) in [
            ("tuple", "tuple"),
            ("conditional", "conditional"),
            ("matching", "matching"),
            ("iteration", "iteration"),
            ("closure", "closure"),
            ("raw", "raw"),
            ("module", "module"),
            ("item", "item"),
            ("import", "import"),
            ("wildcard", "wildcard"),
            ("unexpanded", "unexpanded"),
            ("trait-declaration", "trait declaration"),
        ] {
            if prefix == "endpoints" {
                assert!(document["paths"][format!("/{prefix}/{path}")]["get"].is_object());
            }
            let body = TestClient::get(format!("http://localhost/{prefix}/{path}"))
                .send(&service)
                .await
                .take_string()
                .await
                .expect("handler response body");
            assert_eq!(body, expected, "{prefix}/{path}");
        }
    }
}
