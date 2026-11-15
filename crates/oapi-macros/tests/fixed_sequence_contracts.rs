//! Fixed Rust arrays and tuple fields require their serialized number of items.
#![allow(dead_code)]
use salvo::oapi::{Components, ComposeSchema, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(bound(
    serialize = "[u8; N]: Serialize",
    deserialize = "[u8; N]: Deserialize<'de>"
))]
struct Arrays<const N: usize> {
    data: [u8; 3],
    generic: [u8; N],
}
#[derive(Serialize, Deserialize, ToSchema)]
struct Tuple {
    data: (String, u32),
}
#[derive(Serialize, ToSchema)]
struct DifferentArrays([u8; 2], [u8; 3]);
fn case<T: Serialize + ToSchema>(value: T, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"schema":{"allOf":[schema],"components":components},"valid":[value],"invalid":invalid})
}
#[test]
fn fixed_lengths_match_serde() {
    assert!(serde_json::from_value::<[u8; 3]>(json!([1, 2])).is_err());
    assert!(serde_json::from_value::<Tuple>(json!({"data":["one"]})).is_err());
    let mut components = Components::new();
    let native = json!(<[u8; 3]>::to_schema(&mut components));
    let composed = json!(<[u8; 3]>::compose(&mut components, vec![]));
    for schema in [&native, &composed] {
        assert_eq!(schema["minItems"], 3);
        assert_eq!(schema["maxItems"], 3);
    }
    let cases = vec![
        case(
            Arrays::<2> {
                data: [1, 2, 3],
                generic: [4, 5],
            },
            vec![
                json!({"data":[1,2],"generic":[4,5]}),
                json!({"data":[1,2,3],"generic":[4]}),
                json!({"data":[1,2,3,4],"generic":[4,5]}),
            ],
        ),
        case(
            Tuple {
                data: ("one".into(), 7),
            },
            vec![json!({"data":["one"]}), json!({"data":["one",7,8]})],
        ),
        case([1_u8, 2, 3], vec![json!([1, 2]), json!([1, 2, 3, 4])]),
        case([] as [u8; 0], vec![json!([1])]),
        case(
            DifferentArrays([1, 2], [3, 4, 5]),
            vec![json!([[1, 2], [3, 4]]), json!([[1, 2, 3], [3, 4, 5]])],
        ),
        json!({"schema":composed,"valid":[[1,2,3]],"invalid":[[1,2],[1,2,3,4]]}),
    ];
    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
