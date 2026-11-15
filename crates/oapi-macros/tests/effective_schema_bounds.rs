//! Replaced and skipped field types do not add schema bounds or registrations.
#![allow(dead_code)]
use salvo::oapi::{Components, ComposeSchema, Object, ToSchema};
use serde_json::json;

struct NoSchema;
#[derive(ToSchema)]
#[salvo(schema(value_type = String))]
struct Newtype<T>(T);
#[derive(serde::Serialize, ToSchema)]
enum SkippedVariants<T> {
    Shown,
    #[salvo(schema(skip))]
    Hidden(T),
    #[serde(skip)]
    SerdeHidden {
        value: T,
    },
}
#[derive(ToSchema)]
enum ReplacedVariant<T> {
    #[salvo(schema(value_type = String))]
    Value(T),
}
fn custom() -> Object {
    Object::with_type(salvo::oapi::schema::BasicType::String)
}
#[derive(ToSchema)]
struct Replaced<T> {
    #[salvo(schema(value_type = String))]
    value: T,
}
#[derive(ToSchema)]
struct Custom<T> {
    #[salvo(schema(schema_with = custom))]
    value: T,
}
#[derive(ToSchema)]
struct Skipped<T> {
    value: String,
    #[salvo(schema(skip))]
    hidden: T,
}
#[derive(serde::Serialize, ToSchema)]
struct SerdeSkipped<T> {
    value: String,
    #[serde(skip_serializing)]
    hidden: T,
}
#[derive(ToSchema)]
struct Retarget<T, U> {
    #[salvo(schema(value_type = U))]
    value: T,
    #[salvo(schema(skip))]
    marker: std::marker::PhantomData<U>,
}

#[test]
fn effective_types_supply_only_needed_bounds() {
    let mut components = Components::new();
    let _ = Newtype::<NoSchema>::to_schema(&mut components);
    let _ = SkippedVariants::<NoSchema>::to_schema(&mut components);
    let _ = ReplacedVariant::<NoSchema>::to_schema(&mut components);
    let _ = Newtype::<NoSchema>::compose(&mut components, vec![]);
    let _ = ReplacedVariant::<NoSchema>::compose(&mut components, vec![]);
    let _ = Replaced::<NoSchema>::to_schema(&mut components);
    let _ = Custom::<NoSchema>::to_schema(&mut components);
    let _ = Skipped::<NoSchema>::to_schema(&mut components);
    let _ = SerdeSkipped::<NoSchema>::to_schema(&mut components);
    let _ = Retarget::<NoSchema, String>::to_schema(&mut components);
    for schema in [
        Replaced::<NoSchema>::compose(&mut components, vec![]),
        Custom::<NoSchema>::compose(&mut components, vec![]),
        Retarget::<NoSchema, String>::compose(
            &mut components,
            vec![
                Object::with_type(salvo::oapi::schema::BasicType::Integer).into(),
                Object::with_type(salvo::oapi::schema::BasicType::Boolean).into(),
            ],
        ),
    ] {
        assert!(json!(schema)["properties"]["value"].is_object());
    }
    let schema = json!(Retarget::<NoSchema, String>::compose(
        &mut components,
        vec![
            Object::with_type(salvo::oapi::schema::BasicType::Integer).into(),
            Object::with_type(salvo::oapi::schema::BasicType::String).into()
        ]
    ));
    assert_eq!(schema["properties"]["value"]["type"], "string");
}
