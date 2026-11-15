//! Stateless generic factories retain service scope without retaining service state.
#![deny(deprecated, non_local_definitions, non_snake_case)]

use std::marker::PhantomData;
use std::rc::Rc;

use salvo::oapi::extract::QueryParam;
use salvo::oapi::{OpenApi, ToSchema, endpoint};
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;
use serde_json::{json, to_value};

struct Service<'owner, T, const N: usize>(PhantomData<&'owner T>);

#[craft]
impl<'owner, T, const N: usize> Service<'owner, T, N> {
    const LABEL: &'static str = "service";

    #[craft(handler)]
    fn describe() -> String {
        let _: PhantomData<&'owner T> = PhantomData;
        format!("{}:{N}:{}", Self::LABEL, std::any::type_name::<T>())
    }

    #[craft(handler)]
    fn nested() -> &'static str {
        trait Local {
            const LABEL: &'static str;
            fn label() -> &'static str {
                Self::LABEL
            }
        }
        struct Nested;
        impl Local for Nested {
            const LABEL: &'static str = "nested";
        }
        Nested::label()
    }

    /// A generic static endpoint.
    #[craft(endpoint(
        tags(format!("{}-{N}", Self::LABEL)),
        responses((status_code = 200, body = [u8; N]))
    ))]
    fn documented() -> String {
        format!("{}:{N}", Self::LABEL)
    }

    #[deprecated(note = "use documented")]
    #[craft(endpoint)]
    fn old() -> &'static str {
        "old"
    }
}

struct Typed<T>(PhantomData<T>);
#[craft]
impl<T> Typed<T> {
    #[craft(endpoint)]
    fn value(value: QueryParam<T>) -> Json<T>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + ToSchema + Send,
    {
        Json(value.into_inner())
    }
}

#[endpoint]
fn ordinary() -> &'static str {
    "inventory"
}

#[deprecated(note = "use ordinary")]
#[endpoint]
fn ordinary_old() -> &'static str {
    "ordinary-old"
}

fn borrowed<'short>(_: &'short str) -> impl Handler {
    Service::<'short, &'short str, 3>::describe()
}

fn borrowed_endpoint<'short>(_: &'short str) -> impl Handler {
    Service::<'short, &'short str, 3>::documented()
}

#[allow(non_upper_case_globals)]
struct Reserved<
    const __macro_gen_req: usize,
    const __macro_gen_depot: usize,
    const __macro_gen_res: usize,
    const __macro_gen_ctrl: usize,
    const __macro_gen_conn: usize,
    const __salvo_endpoint_type_id: usize,
>;

#[allow(non_upper_case_globals)]
#[craft]
impl<
    const __macro_gen_req: usize,
    const __macro_gen_depot: usize,
    const __macro_gen_res: usize,
    const __macro_gen_ctrl: usize,
    const __macro_gen_conn: usize,
    const __salvo_endpoint_type_id: usize,
>
    Reserved<
        { __macro_gen_req },
        { __macro_gen_depot },
        { __macro_gen_res },
        { __macro_gen_ctrl },
        { __macro_gen_conn },
        { __salvo_endpoint_type_id },
    >
{
    #[craft(handler)]
    fn handler(conn: &mut ConnCtrl) -> &'static str {
        conn.relax_timeouts();
        "reserved-handler"
    }

    #[craft(endpoint)]
    fn endpoint() -> &'static str {
        "reserved-endpoint"
    }
}

#[tokio::test]
async fn unused_impl_constants_do_not_capture_generated_bindings() {
    let router = Router::new()
        .push(Router::with_path("handler").get(Reserved::<1, 2, 3, 4, 5, 6>::handler()))
        .push(Router::with_path("endpoint").get(Reserved::<1, 2, 3, 4, 5, 6>::endpoint()));
    let doc = to_value(OpenApi::new("reserved constants", "1").merge_router(&router)).unwrap();
    assert!(doc["paths"]["/endpoint"]["get"].is_object());
    let service = salvo::Service::new(router);
    for (path, expected) in [
        ("handler", "reserved-handler"),
        ("endpoint", "reserved-endpoint"),
    ] {
        let mut response = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), expected);
    }
}

#[tokio::test]
async fn bodies_keep_type_const_lifetime_and_nested_self_scopes() {
    let local = String::from("short borrow");
    let cases = [
        (
            Router::new().get(borrowed(&local)),
            "service:3:&str".to_owned(),
        ),
        (
            Router::new().get(Service::<'_, Rc<()>, 5>::describe()),
            format!("service:5:{}", std::any::type_name::<Rc<()>>()),
        ),
        (
            Router::new().get(Service::<'_, (), 2>::nested()),
            "nested".to_owned(),
        ),
    ];
    for (router, expected) in cases {
        let body = TestClient::get("http://localhost/")
            .send(router)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(body, expected);
    }
}

#[tokio::test]
#[allow(deprecated)]
async fn each_erased_endpoint_instance_keeps_its_own_metadata() {
    let local = String::from("short borrow");
    let number = Typed::<u32>::value();
    let text = Typed::<String>::value();
    // The erased wrapper type is shared, so TypeId alone cannot identify its metadata.
    assert_eq!(number.type_id(), text.type_id());
    let router = Router::new()
        .push(Router::with_path("text").get(text))
        .push(Router::with_path("number").get(number))
        .push(Router::with_path("borrowed").get(borrowed_endpoint(&local)))
        .push(Router::with_path("wide").get(Service::<'_, (), 7>::documented()))
        .push(Router::with_path("ordinary").get(ordinary))
        .push(Router::with_path("old").get(Service::<'_, (), 3>::old()))
        .push(Router::with_path("ordinary-old").get(ordinary_old));
    let doc = to_value(OpenApi::new("generic", "1").merge_router(&router)).unwrap();
    assert_eq!(
        doc["paths"]["/text"]["get"]["parameters"][0]["schema"]["type"],
        "string"
    );
    assert_eq!(
        doc["paths"]["/number"]["get"]["parameters"][0]["schema"]["type"],
        "integer"
    );
    assert_eq!(
        doc["paths"]["/text"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["type"],
        "string"
    );
    assert_eq!(
        doc["paths"]["/number"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["type"],
        "integer"
    );
    assert_eq!(
        doc["paths"]["/borrowed"]["get"]["tags"],
        json!(["service-3"])
    );
    assert_eq!(doc["paths"]["/wide"]["get"]["tags"], json!(["service-7"]));
    assert_eq!(
        doc["paths"]["/borrowed"]["get"]["summary"],
        "A generic static endpoint."
    );
    assert!(doc["paths"]["/ordinary"]["get"].is_object());
    assert_eq!(doc["paths"]["/old"]["get"]["deprecated"], true);
    assert_eq!(doc["paths"]["/ordinary-old"]["get"]["deprecated"], true);
    assert!(doc["paths"]["/borrowed"]["get"].get("deprecated").is_none());
    assert!(doc["paths"]["/ordinary"]["get"].get("deprecated").is_none());
    assert_ne!(
        doc["paths"]["/borrowed"]["get"]["operationId"],
        doc["paths"]["/wide"]["get"]["operationId"]
    );

    use salvo::oapi::naming::{FlexNamer, NameRule, Namer, set_namer};
    let short_namer = FlexNamer::new()
        .short_mode(true)
        .generic_delimiter('_', '_');
    set_namer(short_namer.clone());
    let short_doc = to_value(OpenApi::new("short names", "1").merge_router(&router)).unwrap();
    let expected = short_namer.assign_name(
        Typed::<u32>::value().type_id(),
        std::any::type_name_of_val(&Typed::<u32>::value),
        NameRule::Auto,
    );
    assert_eq!(
        short_doc["paths"]["/number"]["get"]["operationId"],
        expected
    );
    assert!(!expected.contains("::"));
    assert!(!expected.contains(['<', '>']));

    struct CustomNamer;
    impl Namer for CustomNamer {
        fn assign_name(&self, _id: std::any::TypeId, name: &'static str, rule: NameRule) -> String {
            match rule {
                NameRule::Auto => format!("configured-{}", name.replace("::", "_")),
                NameRule::Force(name) => name.to_owned(),
            }
        }
    }
    set_namer(CustomNamer);
    let custom_doc = to_value(OpenApi::new("custom names", "1").merge_router(&router)).unwrap();
    for path in ["/text", "/number", "/borrowed", "/wide", "/ordinary"] {
        assert!(
            custom_doc["paths"][path]["get"]["operationId"]
                .as_str()
                .unwrap()
                .starts_with("configured-")
        );
    }
    assert!(
        custom_doc["paths"]["/number"]["get"]["operationId"]
            .as_str()
            .unwrap()
            .contains("Typed<u32>")
    );
    assert!(
        custom_doc["paths"]["/number"]["get"]["operationId"]
            .as_str()
            .unwrap()
            .ends_with("_value")
    );
    set_namer(FlexNamer::new());

    let service = salvo::Service::new(router);
    for (path, expected) in [
        ("number?value=42", "42"),
        ("text?value=hello", "\"hello\""),
        ("borrowed", "service:3"),
        ("wide", "service:7"),
        ("ordinary", "inventory"),
    ] {
        let body = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(body, expected);
    }
}
