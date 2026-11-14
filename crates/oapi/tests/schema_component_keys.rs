//! Default schema names and configured delimiters produce valid component keys.
#![allow(dead_code)]
use salvo_oapi::naming::{FlexNamer, set_namer};
use salvo_oapi::{Components, RefOr, ToSchema};
use serde_json::json;

#[derive(ToSchema)]
struct Wrapper<T> {
    value: T,
}
#[derive(ToSchema)]
struct Nested {
    value: Wrapper<Wrapper<String>>,
}
#[derive(ToSchema)]
#[salvo(schema(name = "Wrap"))]
struct Alias<T> {
    value: T,
}
#[derive(ToSchema)]
#[salvo(schema(name = "Wrap_3CString_3E"))]
struct Literal {
    value: u32,
}
#[derive(ToSchema)]
#[salvo(schema(name = "Legal_Alias-2"))]
struct Legal {
    value: u32,
}
#[derive(ToSchema)]
struct Marker<T> {
    value: u32,
    #[salvo(schema(skip))]
    marker: std::marker::PhantomData<T>,
}

#[test]
fn names_are_valid_unique_and_references_resolve_in_each_naming_mode() {
    for namer in [
        FlexNamer::new(),
        FlexNamer::new().short_mode(true),
        FlexNamer::new().generic_delimiter('_', '_'),
        FlexNamer::new().generic_delimiter('[', ']'),
    ] {
        set_namer(namer);
        let mut components = Components::new();
        let references = [
            Wrapper::<String>::to_schema(&mut components),
            Wrapper::<u32>::to_schema(&mut components),
            Nested::to_schema(&mut components),
            Alias::<String>::to_schema(&mut components),
            Literal::to_schema(&mut components),
            Legal::to_schema(&mut components),
            Marker::<fn(u32) -> u32>::to_schema(&mut components),
        ];
        let document = json!({"components": components});
        for key in document["components"]["schemas"]
            .as_object()
            .unwrap()
            .keys()
        {
            assert!(!key.is_empty());
            assert!(
                key.bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
                "invalid key: {key}"
            );
        }
        for reference in &references {
            let RefOr::Ref(reference) = reference else {
                panic!("expected component reference");
            };
            assert!(document.pointer(&reference.ref_location[1..]).is_some());
        }
        assert_ne!(json!(references[0]), json!(references[1]));
        assert_ne!(json!(references[3]), json!(references[4]));
        assert!(
            document["components"]["schemas"]
                .get("Legal_Alias-2")
                .is_some()
        );
    }
}
