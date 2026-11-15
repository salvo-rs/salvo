//! Recursive inline schemas must terminate without losing generic overrides.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use salvo_core::Router;
use salvo_oapi::{
    BasicType, Components, ComposeSchema, Object, OpenApi, RefOr, Schema, ToSchema, endpoint,
};
use serde_json::{Value, json};

#[derive(ToSchema)]
struct Node<T> {
    value: T,
    children: Vec<Node<T>>,
    next: Option<Box<Link<T>>>,
}
type Link<T> = Node<T>;

#[derive(ToSchema)]
struct SelfNode<T> {
    value: T,
    next: Option<Box<Self>>,
}

#[derive(serde::Serialize, ToSchema)]
struct SkippedRecursive<T> {
    #[serde(skip)]
    marker: std::marker::PhantomData<T>,
    children: Vec<Self>,
}
struct BorrowedMarker<'a>(&'a str);

#[derive(ToSchema)]
#[salvo(schema(inline))]
struct InlineNode<T> {
    value: T,
    next: Option<Box<InlineNode<T>>>,
}

trait RecursiveItem {
    type Item: ToSchema;
}
struct Seed;
impl RecursiveItem for Seed {
    type Item = Option<Box<Associated<Seed>>>;
}
#[derive(ToSchema)]
#[salvo(schema(bound = "T: RecursiveItem"))]
struct Associated<T: RecursiveItem> {
    next: T::Item,
}

#[endpoint(responses((status_code=200, body=inline(Node<String>))))]
fn inline_node() {}

fn document(schema: RefOr<Schema>, components: &Components) -> Value {
    json!({"allOf":[schema],"components":components})
}

fn assert_closed_references(document: &Value) {
    fn walk(value: &Value, document: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                    let target = document
                        .pointer(&reference[1..])
                        .unwrap_or_else(|| panic!("unresolved {reference}: {document}"));
                    assert_ne!(
                        target.get("$ref").and_then(Value::as_str),
                        Some(reference),
                        "unfinished placeholder {reference}"
                    );
                }
                for value in object.values() {
                    walk(value, document);
                }
            }
            Value::Array(values) => {
                for value in values {
                    walk(value, document);
                }
            }
            _ => {}
        }
    }
    walk(document, document);
}

static FAIL_LATE: AtomicBool = AtomicBool::new(false);
fn late_schema() -> RefOr<Schema> {
    assert!(
        !FAIL_LATE.load(Ordering::SeqCst),
        "intentional schema construction failure"
    );
    Object::with_type(BasicType::Null).into()
}

#[derive(ToSchema)]
struct Parent<T> {
    value: T,
    child: Option<Box<Child<T>>>,
    #[salvo(schema(schema_with=late_schema))]
    late: (),
}
#[derive(ToSchema)]
struct Child<T> {
    value: T,
    again: Option<Box<Child<T>>>,
    parent: Option<Box<Parent<T>>>,
}

static CLONE_ONCE: AtomicBool = AtomicBool::new(true);
fn clone_during_construction(components: &mut Components) -> RefOr<Schema> {
    if CLONE_ONCE.swap(false, Ordering::SeqCst) {
        let mut independent = components.clone();
        let root = CloneNode::compose(&mut independent, vec![]);
        assert_closed_references(&document(root, &independent));
    }
    Object::with_type(BasicType::Null).into()
}
struct CloneProbe;
impl ToSchema for CloneProbe {
    fn to_schema(components: &mut Components) -> RefOr<Schema> {
        clone_during_construction(components)
    }
}
impl ComposeSchema for CloneProbe {
    fn compose(components: &mut Components, _: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        clone_during_construction(components)
    }
}
#[derive(ToSchema)]
#[salvo(schema(inline))]
struct CloneNode {
    children: Vec<CloneNode>,
    probe: CloneProbe,
}

fn scenarios() {
    let mut cases = Vec::new();
    fn compose_borrowed<'a>(_: &'a str) {
        let mut components = Components::new();
        let root = SkippedRecursive::<BorrowedMarker<'a>>::compose(&mut components, vec![]);
        assert_closed_references(&document(root, &components));
    }
    let local = "borrowed marker".to_owned();
    compose_borrowed(&local);
    let mut strings = Components::new();
    let root = Node::<String>::compose(&mut strings, vec![]);
    let string_document = document(root.clone(), &strings);
    assert_closed_references(&string_document);
    assert_eq!(strings.schemas.len(), 1);
    assert!(
        string_document["allOf"][0]["properties"]["children"]["items"]
            .get("$ref")
            .is_some()
    );
    let original = strings.clone();
    assert_eq!(
        json!(Node::<String>::compose(&mut strings, vec![])),
        json!(root)
    );
    assert_eq!(
        strings.schemas.len(),
        1,
        "repeated composition reuses the recursive component"
    );
    let imported: Components = serde_json::from_value(json!(strings)).unwrap();
    assert_eq!(strings, original);
    assert_eq!(
        strings, imported,
        "construction state is not part of document equality"
    );
    cases.push(json!({"schema":string_document,"valid":[{"value":"root","children":[{"value":"child","children":[],"next":null}],"next":null}],"invalid":[{"value":"root","children":[{"value":7,"children":[],"next":null}],"next":null}]}));

    let mut booleans = Components::new();
    let boolean_root = Node::<String>::compose(
        &mut booleans,
        vec![Object::with_type(BasicType::Boolean).into()],
    );
    let boolean_document = document(boolean_root.clone(), &booleans);
    assert_closed_references(&boolean_document);
    let valid_boolean = json!({"value":true,"children":[{"value":false,"children":[],"next":null}],"next":{"value":true,"children":[],"next":null}});
    let invalid_boolean =
        json!({"value":true,"children":[{"value":"wrong","children":[],"next":null}],"next":null});
    cases.push(
        json!({"schema":boolean_document,"valid":[valid_boolean],"invalid":[invalid_boolean]}),
    );
    assert_ne!(
        strings.schemas.keys().next(),
        booleans.schemas.keys().next(),
        "different overrides need distinct names across documents"
    );
    strings.append(&mut booleans);
    let merged = document(
        Object::new()
            .property("strings", root)
            .property("booleans", boolean_root)
            .required("strings")
            .required("booleans")
            .into(),
        &strings,
    );
    assert_closed_references(&merged);
    cases.push(json!({"schema":merged,"valid":[{"strings":{"value":"x","children":[],"next":null},"booleans":{"value":true,"children":[],"next":null}}],"invalid":[{"strings":{"value":"x","children":[],"next":null},"booleans":{"value":"wrong","children":[],"next":null}}]}));
    let same_document = Node::<String>::compose(
        &mut strings,
        vec![Object::with_type(BasicType::Boolean).into()],
    );
    assert_closed_references(&document(same_document, &strings));
    assert_eq!(
        strings.schemas.len(),
        3,
        "an appended document's construction cache is independent"
    );
    strings.schemas.clear();
    let rebuilt = Node::<String>::compose(&mut strings, vec![]);
    assert_closed_references(&document(rebuilt, &strings));
    assert_eq!(
        strings.schemas.len(),
        1,
        "removed components invalidate cached construction state"
    );

    let mut self_components = Components::new();
    let self_root = SelfNode::<String>::compose(
        &mut self_components,
        vec![Object::with_type(BasicType::Boolean).into()],
    );
    let self_document = document(self_root, &self_components);
    assert_closed_references(&self_document);
    cases.push(json!({"schema":self_document,"valid":[{"value":true,"next":{"value":false,"next":null}}],"invalid":[{"value":true,"next":{"value":"wrong","next":null}}]}));

    let mut inline_components = Components::new();
    let inline_root = InlineNode::<String>::to_schema(&mut inline_components);
    let inline_document = document(inline_root, &inline_components);
    assert_closed_references(&inline_document);
    cases.push(json!({"schema":inline_document,"valid":[{"value":"root","next":{"value":"child","next":null}}],"invalid":[{"value":"root","next":{"value":7,"next":null}}]}));
    let api = json!(OpenApi::new("recursive", "1").merge_router(&Router::new().get(inline_node)));
    assert_closed_references(&api);
    assert_eq!(
        api["paths"]["/"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["type"],
        "object"
    );

    let mut associated = Components::new();
    let associated_root = Associated::<Seed>::to_schema(&mut associated);
    let associated_document = document(associated_root, &associated);
    assert_closed_references(&associated_document);
    cases.push(json!({"schema":associated_document,"valid":[{"next":{"next":null}}],"invalid":[{"next":{"next":7}}]}));

    let mut recovered = Components::new().add_schema("keep", Object::with_type(BasicType::Integer));
    FAIL_LATE.store(true, Ordering::SeqCst);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Parent::<String>::compose(
            &mut recovered,
            vec![]
        )))
        .is_err()
    );
    assert_eq!(
        recovered.schemas.len(),
        1,
        "failed parent rolls back completed children referencing it"
    );
    assert!(recovered.schemas.contains_key("keep"));
    FAIL_LATE.store(false, Ordering::SeqCst);
    let recovered_root = Child::<String>::compose(&mut recovered, vec![]);
    assert_closed_references(&document(recovered_root, &recovered));

    let mut named = Components::new().add_schema("keep", Object::with_type(BasicType::Integer));
    let stable = Node::<String>::to_schema(&mut named);
    let reference = json!(stable)["$ref"].as_str().unwrap().to_owned();
    let name = reference.strip_prefix("#/components/schemas/").unwrap();
    named.schemas.retain(|candidate, _| candidate != name);
    let stable = Node::<String>::to_schema(&mut named);
    assert_closed_references(&document(stable.clone(), &named));
    let before_failure = json!(named);
    FAIL_LATE.store(true, Ordering::SeqCst);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || Parent::<String>::to_schema(&mut named)
        ))
        .is_err()
    );
    assert_eq!(
        json!(named),
        before_failure,
        "named panic preserves earlier successful schemas and removes new placeholders"
    );
    FAIL_LATE.store(false, Ordering::SeqCst);
    let named_root = Parent::<String>::to_schema(&mut named);
    assert_closed_references(&document(named_root, &named));
    assert_closed_references(&document(stable, &named));

    CLONE_ONCE.store(true, Ordering::SeqCst);
    let mut cloned = Components::new();
    let clone_root = CloneNode::compose(&mut cloned, vec![]);
    assert_closed_references(&document(clone_root, &cloned));
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}

#[test]
fn recursive_composition_contracts_in_child_process() {
    const CHILD: &str = "SALVO_RECURSIVE_COMPOSITION_CHILD";
    if std::env::var_os(CHILD).is_some() {
        scenarios();
        return;
    }
    // A broken recursion guard aborts on stack overflow. Isolate that abort so
    // the normal test runner reports the child status and captured diagnostics.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "recursive_composition_contracts_in_child_process",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "recursive schema child failed: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
