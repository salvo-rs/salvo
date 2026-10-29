//! Preserve path-level metadata when composing separately built API documents.

use salvo_oapi::{OpenApi, Operation, PathItem, PathItemType, PropMap};
use serde_json::{Value, json};

fn item(method: PathItemType, extensions: &[(&str, Value)]) -> PathItem {
    PathItem::new(method, Operation::new()).extensions(
        extensions
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<PropMap<_, _>>(),
    )
}

fn assert_merged_path(api: OpenApi, path: &str) {
    let value = serde_json::to_value(api).unwrap();
    let path = &value["paths"][path];
    assert!(path.get("get").is_some());
    assert!(path.get("post").is_some());
    assert_eq!(path["x-existing"], json!(true));
    assert_eq!(path["x-incoming"], json!({ "owner": "orders" }));
    // Match PathItem::append's existing collision policy for extensions.
    assert_eq!(path["x-shared"], json!("existing"));
}

#[test]
fn merging_documents_keeps_extensions_on_an_existing_path() {
    let base = OpenApi::new("API", "1").add_path(
        "/items",
        item(
            PathItemType::Get,
            &[("x-existing", json!(true)), ("x-shared", json!("existing"))],
        ),
    );
    let other = OpenApi::new("Orders", "1").add_path(
        "/items",
        item(
            PathItemType::Post,
            &[
                ("x-incoming", json!({ "owner": "orders" })),
                ("x-shared", json!("incoming")),
            ],
        ),
    );
    assert_merged_path(base.merge(other), "/items");
}

#[test]
fn nesting_documents_keeps_extensions_on_a_colliding_path() {
    let base = OpenApi::new("API", "1").add_path(
        "/api/items",
        item(
            PathItemType::Get,
            &[("x-existing", json!(true)), ("x-shared", json!("existing"))],
        ),
    );
    let other = OpenApi::new("Orders", "1").add_path(
        "/items",
        item(
            PathItemType::Post,
            &[
                ("x-incoming", json!({ "owner": "orders" })),
                ("x-shared", json!("incoming")),
            ],
        ),
    );
    assert_merged_path(base.nest("/api", other), "/api/items");
}
