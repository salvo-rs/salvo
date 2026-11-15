//! Qualified user types keep their schema across dependency feature choices.
#![allow(dead_code)]
use salvo_oapi::{Components, RefOr, ToSchema};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize, ToSchema)]
struct Record<T> {
    item: T,
}
mod domainvec {
    pub(crate) type Vec<T> = super::Record<T>;
}
mod domain {
    #[derive(serde::Serialize, salvo_oapi::ToSchema)]
    pub(crate) struct Duration {
        pub(crate) seconds: i64,
    }
}
#[derive(Serialize, ToSchema)]
struct Container {
    custom: domainvec::Vec<u32>,
    duration: domain::Duration,
    standard: std::vec::Vec<u32>,
}

#[test]
fn explicit_user_paths_are_not_reinterpreted_as_builtins() {
    let value = Container {
        custom: Record { item: 7 },
        duration: domain::Duration { seconds: 7 },
        standard: vec![7],
    };
    let mut components = Components::new();
    let RefOr::Ref(reference) = Container::to_schema(&mut components) else {
        panic!("named schema")
    };
    let document = json!({"components":components});
    let schema = document.pointer(&reference.ref_location[1..]).unwrap();
    for name in ["custom", "duration"] {
        let property = &schema["properties"][name];
        let resolved = property
            .get("$ref")
            .and_then(Value::as_str)
            .map(|r| document.pointer(&r[1..]).unwrap())
            .unwrap_or(property);
        assert_eq!(resolved["type"], "object", "{name}");
    }
    assert_eq!(schema["properties"]["standard"]["type"], "array");
    assert_eq!(
        json!(value),
        json!({"custom":{"item":7},"duration":{"seconds":7},"standard":[7]})
    );
}
