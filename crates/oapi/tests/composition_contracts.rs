//! Derived schemas compose actual Rust types and preserve local annotations.
#![allow(dead_code, unused_parens)]
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_core::{Depot, Router};
use salvo_oapi::{
    BasicType, Components, ComposeSchema, Object, OpenApi, RefOr, Schema, ToSchema, endpoint,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn schema<T: ToSchema>() -> Value {
    let mut components = Components::new();
    let root = T::to_schema(&mut components);
    let doc = json!({"root":root,"components":components});
    let mut result = doc["root"]["$ref"]
        .as_str()
        .map(|r| doc.pointer(&r[1..]).unwrap().clone())
        .unwrap_or_else(|| doc["root"].clone());
    result
        .as_object_mut()
        .unwrap()
        .insert("components".into(), doc["components"].clone());
    result
}

#[derive(ToSchema)]
struct Inner<T> {
    value: T,
}
#[derive(ToSchema)]
struct Outer<T> {
    inner: Inner<T>,
}
struct ComposeOnly;
impl ComposeSchema for ComposeOnly {
    fn compose(_: &mut Components, _: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        Object::with_type(BasicType::Integer).into()
    }
}

#[test]
fn nested_generic_composition_propagates_overrides_without_to_schema_bounds() {
    let mut components = Components::new();
    for value in [
        Outer::<String>::compose(
            &mut components,
            vec![Object::with_type(BasicType::Integer).into()],
        ),
        Outer::<ComposeOnly>::compose(&mut components, vec![]),
    ] {
        assert_eq!(
            json!(value)["properties"]["inner"]["properties"]["value"]["type"],
            "integer"
        );
    }
    assert!(
        schema::<Outer<String>>()["properties"]["inner"]
            .get("$ref")
            .is_some()
    );
}

#[derive(ToSchema)]
struct Pair<A, B> {
    left: A,
    right: B,
}
type Swapped<A, B> = Pair<B, A>;
type FixedParameter<A> = Pair<u32, A>;
type FixedString<A> = Pair<String, A>;

#[derive(ToSchema)]
struct Aliased<T> {
    swapped: Swapped<T, u32>,
    fixed: FixedParameter<T>,
    nested: Inner<Vec<T>>,
}

#[test]
fn aliases_reorder_and_fix_parameters_using_the_actual_implementation() {
    for kind in [BasicType::String, BasicType::Boolean] {
        let result = json!(Aliased::<String>::compose(
            &mut Components::new(),
            vec![Object::with_type(kind.clone()).into()]
        ));
        for property in ["swapped", "fixed"] {
            assert_eq!(
                result["properties"][property]["properties"]["left"]["type"],
                "integer"
            );
            assert_eq!(
                result["properties"][property]["properties"]["right"]["type"],
                json!(kind)
            );
        }
        assert_eq!(
            result["properties"]["nested"]["properties"]["value"]["items"]["type"],
            json!(kind)
        );
    }
}

mod opaque_import {
    use super::Pair;
    #[derive(salvo_oapi::ToSchema)]
    pub(crate) struct Wrapper<T, U> {
        value: Pair<T, U>,
    }
}

mod relative_path {
    #[derive(salvo_oapi::ToSchema)]
    pub(crate) struct Wrapper<T, U> {
        value: super::Pair<T, U>,
    }
}

mod same_name_alias {
    type Pair<A, B> = super::Pair<B, A>;
    #[derive(salvo_oapi::ToSchema)]
    pub(crate) struct Wrapper<T, U> {
        value: Pair<T, U>,
    }
}

#[derive(ToSchema)]
struct Repeated<T, U> {
    direct: Pair<T, U>,
    explicit_self: self::Pair<T, U>,
    qualified: crate::Pair<T, U>,
    fixed_direct: Pair<T, String>,
    alias: Swapped<T, U>,
    fixed_alias: FixedString<T>,
    imported: opaque_import::Wrapper<T, U>,
    relative: relative_path::Wrapper<T, U>,
    same_name: same_name_alias::Wrapper<T, U>,
}

#[test]
fn proven_declarations_keep_positions_and_opaque_duplicate_types_fall_back() {
    let result = json!(Repeated::<String, String>::compose(
        &mut Components::new(),
        vec![
            Object::with_type(BasicType::Boolean).into(),
            Object::with_type(BasicType::Integer).into(),
        ]
    ));
    for property in ["direct", "qualified", "explicit_self"] {
        assert_eq!(
            result["properties"][property]["properties"]["left"]["type"],
            "boolean"
        );
        assert_eq!(
            result["properties"][property]["properties"]["right"]["type"],
            "integer"
        );
    }
    assert_eq!(
        result["properties"]["fixed_direct"]["properties"]["left"]["type"],
        "boolean"
    );
    assert_eq!(
        result["properties"]["fixed_direct"]["properties"]["right"]["type"],
        "string"
    );
    for property in ["alias", "fixed_alias"] {
        for field in ["left", "right"] {
            assert_eq!(
                result["properties"][property]["properties"][field]["type"],
                "string"
            );
        }
    }
    for field in ["left", "right"] {
        assert_eq!(
            result["properties"]["imported"]["properties"]["value"]["properties"][field]["type"],
            "string"
        );
        assert_eq!(
            result["properties"]["same_name"]["properties"]["value"]["properties"][field]["type"],
            "string"
        );
    }
    assert_eq!(
        result["properties"]["relative"]["properties"]["value"]["properties"]["left"]["type"],
        "boolean"
    );
    assert_eq!(
        result["properties"]["relative"]["properties"]["value"]["properties"]["right"]["type"],
        "integer"
    );
}

#[derive(ToSchema)]
struct WithConst<T, const N: usize> {
    value: T,
    data: [u8; N],
}
#[derive(ToSchema)]
struct ConstOuter<T> {
    inner: WithConst<T, LIMIT>,
}

#[test]
fn nested_named_constants_are_not_guessed_to_be_schema_types() {
    let result = json!(ConstOuter::<String>::compose(
        &mut Components::new(),
        vec![Object::with_type(BasicType::Boolean).into(),]
    ));
    assert_eq!(
        result["properties"]["inner"]["properties"]["value"]["type"],
        "boolean"
    );
    assert_eq!(
        result["properties"]["inner"]["properties"]["data"]["maxItems"],
        LIMIT
    );
}

#[test]
fn manual_compose_implementations_keep_the_actual_type_default() {
    assert_eq!(
        json!(ComposeOnly::compose_with_bindings(
            &mut Components::new(),
            module_path!(),
            &["UnknownAlias"],
            vec![]
        ))["type"],
        "integer"
    );
}

type Alias = Inner<String>;
const LIMIT: usize = 3;
#[derive(ToSchema)]
struct Bounded<const N: usize> {
    data: [u8; N],
}
#[endpoint(responses((status_code=200,body=inline(Alias))))]
fn alias() {}
#[endpoint(responses((status_code=200,body=inline(Bounded<LIMIT>))))]
fn constant() {}

#[test]
fn inline_alias_and_named_const_resolve_from_the_actual_type() {
    let routes = Router::new()
        .push(Router::with_path("alias").get(alias))
        .push(Router::with_path("const").get(constant));
    let doc = json!(OpenApi::new("test", "1").merge_router(&routes));
    let a =
        &doc["paths"]["/alias"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
    let b =
        &doc["paths"]["/const"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
    assert_eq!(a["properties"]["value"]["type"], "string");
    assert_eq!(b["properties"]["data"]["minItems"], 3);
    assert_eq!(b["properties"]["data"]["maxItems"], 3);
}

mod domain {
    #[derive(serde::Serialize, salvo_oapi::ToSchema)]
    pub(crate) struct Object(pub(crate) String);
}
#[derive(Serialize, ToSchema)]
struct Qualified {
    value: (domain::Object),
    nested: Option<(String)>,
}

#[test]
fn qualified_object_and_parenthesized_types_keep_their_actual_schema() {
    assert_eq!(
        json!(Qualified {
            value: domain::Object("text".into()),
            nested: None
        }),
        json!({"value":"text","nested":null})
    );
    let result = schema::<Qualified>();
    assert!(result["properties"]["value"].get("$ref").is_some());
    assert_eq!(
        result["properties"]["nested"]["type"],
        json!(["string", "null"])
    );
}

#[derive(Serialize, ToSchema)]
struct Child {
    value: String,
}
#[derive(ToSchema)]
struct Annotated<T> {
    #[salvo(schema(read_only, example = "primitive example"))]
    primitive: String,
    #[salvo(schema(read_only,deprecated,example=json!({"value":"example"}))) ]
    child: Child,
    #[salvo(schema(inline, write_only, deprecated, example = json!({"value":"inline"}))) ]
    inline_child: Child,
    #[salvo(schema(
        write_only,
        content_encoding = "base64",
        content_media_type = "text/plain"
    ))]
    any: Value,
    #[salvo(schema(read_only, deprecated, example = "generic example"))]
    generic: T,
}

#[test]
fn ref_value_and_composed_generic_annotations_stay_on_the_property() {
    let mut components = Components::new();
    let root = Annotated::<String>::compose(
        &mut components,
        vec![Object::with_type(BasicType::String).into()],
    );
    let result = json!(root);
    assert_eq!(result["properties"]["primitive"]["readOnly"], true);
    assert_eq!(
        result["properties"]["primitive"]["examples"],
        json!(["primitive example"])
    );
    assert_eq!(result["properties"]["child"]["readOnly"], true);
    assert_eq!(result["properties"]["child"]["deprecated"], true);
    assert_eq!(
        result["properties"]["child"]["examples"],
        json!([{"value":"example"}])
    );
    assert_eq!(result["properties"]["any"]["writeOnly"], true);
    assert_eq!(result["properties"]["any"]["contentEncoding"], "base64");
    assert_eq!(
        result["properties"]["any"]["contentMediaType"],
        "text/plain"
    );
    assert_eq!(result["properties"]["generic"]["readOnly"], true);
    assert_eq!(result["properties"]["inline_child"]["writeOnly"], true);
    assert_eq!(result["properties"]["inline_child"]["deprecated"], true);
    assert_eq!(
        result["properties"]["inline_child"]["examples"],
        json!([{"value":"inline"}])
    );
    assert_eq!(result["properties"]["inline_child"]["type"], "object");
    assert_eq!(
        result["properties"]["generic"]["examples"],
        json!(["generic example"])
    );
    let child = json!(components)["schemas"]
        .as_object()
        .unwrap()
        .values()
        .find(|s| s["properties"].get("value").is_some())
        .unwrap()
        .clone();
    assert!(child.get("readOnly").is_none());
}

#[derive(ToSchema)]
struct ExactNumbers {
    #[salvo(schema(
        maximum = 9007199254740993,
        minimum = 9007199254740993,
        exclusive_maximum = 9007199254740993,
        exclusive_minimum = 9007199254740993,
        multiple_of = 9007199254740993
    ))]
    value: u64,
    #[salvo(schema(minimum=-9223372036854775808))]
    negative: i64,
}

#[test]
fn integer_constraints_do_not_round_through_floating_point() {
    let result = schema::<ExactNumbers>();
    for key in [
        "maximum",
        "minimum",
        "exclusiveMaximum",
        "exclusiveMinimum",
        "multipleOf",
    ] {
        assert_eq!(
            result["properties"]["value"][key],
            json!(9_007_199_254_740_993_u64),
            "{key}"
        );
    }
    assert_eq!(result["properties"]["negative"]["minimum"], json!(i64::MIN));
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "body")))]
struct SkippedSources {
    #[salvo(extract(source(from = "query")))]
    term: String,
    #[serde(skip_deserializing)]
    #[salvo(extract(source(from = "body")))]
    ignored: String,
}

#[tokio::test]
async fn skipped_and_overridden_body_sources_do_not_consume_the_payload() {
    for (content_type, body) in [
        ("application/x-www-form-urlencoded", "ignored=large-payload"),
        ("application/json", r#"{"ignored":"large-payload"}"#),
    ] {
        let mut req = TestClient::post("http://localhost/?term=hello")
            .add_header("content-type", content_type, true)
            .body(body)
            .build();
        req.set_secure_max_size(1);
        let value = SkippedSources::extract(&mut req, &mut Depot::new())
            .await
            .unwrap();
        assert_eq!(value.term, "hello");
        assert_eq!(value.ignored, "");
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct SkippedHeaders {
    #[serde(skip_deserializing)]
    #[salvo(extract(source(from = "header")))]
    ignored: String,
    #[serde(skip_serializing)]
    kept: String,
}

#[tokio::test]
async fn skipped_headers_are_not_decoded_but_serialize_only_skips_are_extracted() {
    let mut req = TestClient::get("http://localhost/?kept=hello").build();
    req.headers_mut().insert(
        "ignored",
        salvo_core::http::header::HeaderValue::from_bytes(&[0xff]).unwrap(),
    );
    let value = SkippedHeaders::extract(&mut req, &mut Depot::new())
        .await
        .unwrap();
    assert_eq!(value.ignored, "");
    assert_eq!(value.kept, "hello");
}

#[cfg(feature = "smallvec")]
#[test]
fn smallvec_array_capacity_is_not_a_json_element_or_length_constraint() {
    type Storage = [u8; 4];
    type ChildStorage = [Child; 2];
    #[derive(Serialize, ToSchema)]
    struct Values {
        values: smallvec::SmallVec<[u8; 4]>,
        aliases: smallvec::SmallVec<Storage>,
        #[salvo(schema(inline))]
        children: smallvec::SmallVec<[Child; 2]>,
        #[salvo(schema(inline))]
        child_aliases: smallvec::SmallVec<ChildStorage>,
    }
    let direct = schema::<smallvec::SmallVec<[u8; 4]>>();
    let composed = json!(smallvec::SmallVec::<[u8; 4]>::compose(
        &mut Components::new(),
        vec![]
    ));
    let derived = schema::<Values>();
    for result in [
        &direct,
        &composed,
        &derived["properties"]["values"],
        &derived["properties"]["aliases"],
    ] {
        assert_eq!(result["items"]["type"], "integer");
        assert!(result.get("maxItems").is_none());
    }
    assert_eq!(derived["properties"]["children"]["items"]["type"], "object");
    assert_eq!(
        derived["properties"]["child_aliases"]["items"]["type"],
        "object"
    );
}

#[derive(Serialize, Deserialize, ToSchema)]
struct TypedMap {
    fixed: u32,
    #[serde(flatten)]
    extras: std::collections::BTreeMap<bool, String>,
}
#[derive(Serialize, Deserialize, ToSchema)]
struct Fixed {
    label: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
struct NestedMap {
    #[serde(flatten)]
    fixed: Fixed,
    #[serde(flatten)]
    extras: std::collections::BTreeMap<bool, String>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, ToSchema)]
enum ExtraKey {
    Extra,
    Other,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
enum Branch {
    A { a: String },
    B { b: bool },
}
#[derive(Serialize, Deserialize, ToSchema)]
struct ConditionalMap {
    id: u32,
    #[serde(flatten)]
    branch: Branch,
    #[serde(flatten)]
    extras: std::collections::BTreeMap<ExtraKey, u32>,
}

#[test]
fn typed_flattened_map_keys_constrain_only_unclaimed_names() {
    let mut cases = Vec::new();
    for (schema, valid, invalid) in [
        (
            schema::<TypedMap>(),
            vec![json!({"fixed":7}), json!({"fixed":7,"true":"yes"})],
            vec![json!({"fixed":7,"other":"yes"})],
        ),
        (
            schema::<NestedMap>(),
            vec![json!({"label":"x"}), json!({"label":"x","false":"no"})],
            vec![json!({"label":"x","other":"yes"})],
        ),
        (
            schema::<ConditionalMap>(),
            vec![
                json!({"id":1,"kind":"A","a":"text","Extra":2}),
                json!({"id":1,"kind":"B","b":true}),
            ],
            vec![
                json!({"id":1,"kind":"A","a":"text","b":true}),
                json!({"id":1,"kind":"A","a":"text","unknown":2}),
                json!({"id":1,"kind":"A","a":"text","Extra":"bad"}),
            ],
        ),
    ] {
        cases.push(json!({"schema":schema,"valid":valid,"invalid":invalid}));
    }
    assert!(cases[0]["schema"].get("propertyNames").is_some());
    assert_eq!(
        json!(TypedMap {
            fixed: 7,
            extras: [(true, "yes".to_owned())].into()
        }),
        cases[0]["valid"][1]
    );
    assert!(serde_json::from_value::<TypedMap>(cases[0]["invalid"][0].clone()).is_err());
    // Serde's flattened-enum reader need not claim fields before the map reads
    // them. Validate the actual serialized response shape for this combination.
    assert_eq!(
        json!(ConditionalMap {
            id: 1,
            branch: Branch::A { a: "text".into() },
            extras: [(ExtraKey::Extra, 2)].into(),
        }),
        cases[2]["valid"][0]
    );
    if let Ok(path) = std::env::var("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}

#[cfg(feature = "indexmap")]
#[test]
fn indexmap_compose_retains_key_constraints() {
    let composed = json!(indexmap::IndexMap::<bool, String>::compose(
        &mut Components::new(),
        vec![]
    ));
    assert_eq!(composed["propertyNames"]["enum"], json!(["true", "false"]));
}
