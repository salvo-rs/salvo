//! Automatically assigned operation IDs are unique at the document level.
use std::collections::BTreeSet;

use salvo_core::Router;
use salvo_oapi::{OpenApi, endpoint};
use serde_json::{Value, json};

#[endpoint]
fn shared() -> &'static str {
    "ok"
}
#[endpoint(operation_id = "explicit-id")]
fn explicit() -> &'static str {
    "ok"
}

fn ids(document: &OpenApi) -> Vec<String> {
    let document = json!(document);
    document["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|item| item.as_object().unwrap().values())
        .filter_map(|op| op.get("operationId").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}
fn assert_unique(document: &OpenApi, count: usize) {
    let names = ids(document);
    assert_eq!(names.len(), count);
    assert_eq!(
        names.iter().collect::<BTreeSet<_>>().len(),
        count,
        "{names:?}"
    );
}

#[test]
fn reused_handlers_across_paths_methods_and_merged_documents_are_unique() {
    let router = Router::new()
        .push(Router::with_path("one").get(shared).post(shared))
        .push(Router::with_path("two").get(shared));
    let document = OpenApi::new("test", "1").merge_router(&router);
    assert_unique(&document, 3);
    assert_eq!(
        json!(document),
        json!(OpenApi::new("test", "1").merge_router(&router))
    );
    let one = OpenApi::new("test", "1").merge_router(&Router::with_path("first").get(shared));
    let two = OpenApi::new("test", "1").merge_router(&Router::with_path("second").get(shared));
    assert_unique(&one.clone().merge(two.clone()), 2);
    assert_unique(&one.nest("/nested", two), 2);
}

#[test]
fn explicit_ids_and_single_use_names_remain_stable() {
    let single = OpenApi::new("test", "1").merge_router(&Router::new().get(shared));
    assert!(ids(&single)[0].ends_with("shared"));
    let document = single
        .merge_router_with_base(&Router::new().get(shared), "/other")
        .merge_router(&Router::with_path("explicit").get(explicit));
    assert_unique(&document, 3);
    assert_eq!(
        json!(document)["paths"]["/explicit"]["get"]["operationId"],
        "explicit-id"
    );
    let roundtrip: OpenApi = serde_json::from_value(json!(document)).unwrap();
    assert_eq!(document, roundtrip);
}

#[test]
fn explicit_and_directly_overridden_ids_are_reserved_before_automatic_ids() {
    use salvo_oapi::{Operation, PathItem, PathItemType, Response};
    let mut document = OpenApi::new("test", "1").merge_router(&Router::new().get(shared));
    let base = ids(&document)[0].clone();
    let mut operation = Operation::new().operation_id(base.clone());
    operation.responses.insert("200", Response::new("Ok"));
    document.paths.insert(
        "/manual".to_owned(),
        PathItem::new(PathItemType::Get, operation),
    );
    let mut document = document.merge_router(&Router::with_path("other").get(shared));
    assert_unique(&document, 3);
    assert_eq!(
        json!(document)["paths"]["/manual"]["get"]["operationId"],
        base
    );
    document
        .paths
        .get_mut("/")
        .unwrap()
        .operations
        .get_mut(&PathItemType::Get)
        .unwrap()
        .operation_id = Some("direct-override".to_owned());
    let document = document.merge_router(&Router::with_path("last").get(shared));
    assert_unique(&document, 4);
    assert_eq!(
        json!(document)["paths"]["/"]["get"]["operationId"],
        "direct-override"
    );
}
