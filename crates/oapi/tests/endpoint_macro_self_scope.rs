//! Metadata expressions expand macros in the endpoint owner's scope.
#![deny(non_local_definitions)]

use salvo_core::http::StatusCode;
use salvo_core::writing::Json;
use salvo_core::{Router, Scribe};
use salvo_oapi::{OpenApi, ToResponse, endpoint};
use serde_json::json;

struct Service;

// The metadata helper's private trait must not shadow this owner.
#[allow(non_camel_case_types)]
struct EndpointMetadata;
trait UserOperationId {
    fn create() -> &'static str;
}
impl UserOperationId for EndpointMetadata {
    fn create() -> &'static str {
        "trait-name-owner"
    }
}
#[endpoint(operation_id = format!("{}", Self::create()))]
impl EndpointMetadata {
    fn handle(&self) -> &'static str {
        "trait-name-owner"
    }
}

macro_rules! status {
    () => {
        Self::CODE
    };
}

#[endpoint(
    operation_id = format!("{}-{}", Self::ID, {
        trait Local { const ID: &'static str; fn id() -> &'static str { Self::ID } }
        struct Nested;
        impl Local for Nested { const ID: &'static str = "nested"; }
        Nested::id()
    }),
    tags(format!("tag-{}", Self::ID)),
    status_codes(status!()),
    responses((status_code = 200, body = [u8; Self::WIDTH], content_type = "application/json", example = json!([Self::VALUE, Self::VALUE])))
)]
impl Service {
    const ID: &'static str = "service";
    const WIDTH: usize = 2;
    const VALUE: u8 = 7;
    const CODE: StatusCode = StatusCode::OK;

    fn handle(&self) -> Json<[u8; Self::WIDTH]> {
        Json([Self::VALUE; Self::WIDTH])
    }
}

#[derive(ToResponse)]
struct Owner;

impl Scribe for Owner {
    fn render(self, res: &mut salvo_core::Response) {
        res.render("owner");
    }
}

#[endpoint(operation_id = format!("{}", Self::ID))]
impl Owner {
    const ID: &'static str = "owner";
    fn handle(&self) -> Self {
        Self
    }
}

#[test]
fn macros_and_nested_items_keep_their_self_bindings() {
    let router = Router::new()
        .push(Router::with_path("service").get(Service))
        .push(Router::with_path("owner").get(Owner))
        .push(Router::with_path("trait-name").get(EndpointMetadata));
    let doc = json!(OpenApi::new("scope", "1").merge_router(&router));
    assert_eq!(
        doc["paths"]["/trait-name"]["get"]["operationId"],
        "trait-name-owner"
    );
    let operation = &doc["paths"]["/service"]["get"];
    assert_eq!(operation["operationId"], "service-nested");
    assert_eq!(operation["tags"], json!(["tag-service"]));
    assert_eq!(
        operation["responses"]["200"]["content"]["application/json"]["example"],
        json!([7, 7])
    );
    assert_eq!(doc["paths"]["/owner"]["get"]["operationId"], "owner");
}
