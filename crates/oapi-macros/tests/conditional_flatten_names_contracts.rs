//! Conditional flattened fields must not weaken the remaining map's key or value rules.
#![allow(dead_code)]

use std::collections::BTreeMap;

use salvo::oapi::schema::BasicType;
use salvo::oapi::{Components, ComposeSchema, Object, Ref, RefOr, Schema, ToSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Serialize, ToSchema)]
#[serde(tag = "kind")]
enum Tagged {
    A { a: String },
    B { b: bool },
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(untagged)]
enum Untagged {
    A { a: String },
    B { b: bool },
}

#[derive(Serialize, ToSchema)]
#[serde(untagged)]
enum GuardedUntagged {
    A { a: String, a_only: String },
    B { b: bool },
}

#[derive(Clone, Serialize, ToSchema)]
struct Parent<B, T> {
    id: String,
    #[serde(flatten)]
    branch: B,
    #[serde(flatten)]
    values: T,
}

#[derive(Serialize, ToSchema)]
struct OptionalBranch<B, T> {
    id: String,
    #[serde(flatten)]
    branch: Option<B>,
    #[serde(flatten)]
    values: T,
}

#[derive(Serialize, ToSchema)]
struct OptionalValues<B, T> {
    id: String,
    #[serde(flatten)]
    branch: B,
    #[serde(flatten)]
    values: Option<T>,
}

#[derive(Serialize, ToSchema)]
struct Nested<T> {
    note: String,
    #[serde(flatten)]
    inner: T,
}

#[derive(Serialize, ToSchema)]
struct NestedAlias<T> {
    a: String,
    #[serde(flatten)]
    inner: T,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct OptionalMap<T> {
    id: String,
    #[serde(flatten)]
    values: Option<T>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum MapKey {
    Extra,
    Other,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
struct EnumMap(BTreeMap<MapKey, u32>);

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
struct PatternMap(BTreeMap<String, u32>);

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
struct ClosedMap(BTreeMap<String, u32>);

fn enum_names(components: &mut Components) -> RefOr<Schema> {
    MapKey::to_schema(components)
}

fn pattern_names(_: &mut Components) -> RefOr<Schema> {
    Object::with_type(BasicType::String).pattern("^x_").into()
}

fn closed_names(_: &mut Components) -> RefOr<Schema> {
    false.into()
}

macro_rules! map_schema {
    ($ty:ty, $names:path) => {
        impl ComposeSchema for $ty {
            fn compose(components: &mut Components, _: Vec<RefOr<Schema>>) -> RefOr<Schema> {
                let names = $names(components);
                let values = u32::to_schema(components);
                Object::new()
                    .property_names(names)
                    .additional_properties(values)
                    .into()
            }
        }
        impl ToSchema for $ty {
            fn to_schema(components: &mut Components) -> RefOr<Schema> {
                let schema = Self::compose(components, vec![]);
                components.schemas.insert(stringify!($ty), schema);
                Ref::from_schema_name(stringify!($ty)).into()
            }
        }
    };
}

map_schema!(EnumMap, enum_names);
map_schema!(PatternMap, pattern_names);
map_schema!(ClosedMap, closed_names);

fn case<T: Serialize + ToSchema>(name: &str, values: Vec<T>, invalid: Vec<Value>) -> Value {
    let mut components = Components::new();
    let schema = T::to_schema(&mut components);
    json!({"name":name,"schema":{"allOf":[schema],"components":components},"valid":values,"invalid":invalid})
}

fn invalid(tagged: bool, extra_key: Option<&str>) -> Vec<Value> {
    let mut base = json!({"id":"one","a":"yes"});
    if tagged {
        base["kind"] = json!("A");
    }
    let mut invalid = Vec::new();
    for (key, value) in [
        ("id", json!(2)),
        ("a", json!(2)),
        ("b", json!(2)),
        ("rogue", json!(2)),
    ] {
        let mut value_with_error = base.clone();
        value_with_error[key] = value;
        invalid.push(value_with_error);
    }
    if let Some(key) = extra_key {
        let mut bad_value = base.clone();
        bad_value[key] = json!("bad");
        invalid.push(bad_value);
    }
    if tagged {
        base["kind"] = json!("missing");
        invalid.push(base);
    }
    invalid
}

fn composed_case<B: ToSchema + ComposeSchema, T: ToSchema + ComposeSchema>(
    name: &str,
    inline: bool,
    valid: Vec<Value>,
    invalid: Vec<Value>,
) -> Value {
    let mut components = Components::new();
    let branch = if inline {
        B::compose(&mut components, vec![])
    } else {
        B::to_schema(&mut components)
    };
    let values = if inline {
        T::compose(&mut components, vec![])
    } else {
        T::to_schema(&mut components)
    };
    let original = components.schemas.clone();
    let schema = Parent::<B, T>::compose(&mut components, vec![branch, values]);
    assert_eq!(
        components.schemas, original,
        "composition modified reusable components"
    );
    json!({"name":name,"schema":{"allOf":[schema],"components":components},"valid":valid,"invalid":invalid})
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind")]
enum NumericBranch {
    A {
        #[serde(rename = "12")]
        twelve: String,
    },
    B {
        b: bool,
    },
}

fn synthetic_case(name: &str, items: Vec<Value>, valid: Vec<Value>, invalid: Vec<Value>) -> Value {
    let schema = flatten_json(items, &Components::new());
    json!({"name":name,"schema":schema,"valid":valid,"invalid":invalid})
}

#[test]
fn cyclic_conditional_guards_remain_opaque() {
    let mut components = Components::new();
    components.schemas.insert(
        "Cycle",
        Object::new().add_extension("if", json!({"not": {"$ref": "#/components/schemas/Cycle"}})),
    );
    let original = components.schemas.clone();
    let input = salvo::oapi::schema::AllOf::new()
        .item(Ref::from_schema_name("Cycle"))
        .item(Object::new().property("id", Object::with_type(BasicType::String)));
    let schema = salvo::oapi::schema::flatten_all_of_schema(input, &components);
    assert_eq!(components.schemas, original);
    assert_eq!(
        serde_json::to_value(schema).unwrap()["allOf"][0]["$ref"],
        "#/components/schemas/Cycle"
    );
}

fn flatten_json(items: Vec<Value>, components: &Components) -> Value {
    let mut all = salvo::oapi::schema::AllOf::new();
    for item in items {
        all = all.item(serde_json::from_value::<RefOr<Schema>>(item).unwrap());
    }
    json!(salvo::oapi::schema::flatten_all_of_schema(all, components))
}

#[test]
fn conditional_names_follow_the_successful_branches() {
    let mut cases = Vec::new();
    macro_rules! add_map_cases {
        ($ty:ty, $map:expr, $key:expr, $label:literal) => {{
            let map: $ty = $map;
            let tagged = vec![
                Parent { id: "one".into(), branch: Tagged::A { a: "yes".into() }, values: map.clone() },
                Parent { id: "one".into(), branch: Tagged::B { b: true }, values: map.clone() },
            ];
            let untagged = vec![
                Parent { id: "one".into(), branch: Untagged::A { a: "yes".into() }, values: map.clone() },
                Parent { id: "one".into(), branch: Untagged::B { b: true }, values: map.clone() },
            ];
            // Serde's flatten deserializer need not consume fields read by an enum.
            // These contracts intentionally use the real Serialize output instead.
            let tagged_json: Vec<Value> = tagged.iter().map(|value| serde_json::to_value(value).unwrap()).collect();
            let untagged_json: Vec<Value> = untagged.iter().map(|value| serde_json::to_value(value).unwrap()).collect();
            cases.push(case(concat!($label, "-one-of"), tagged, invalid(true, $key)));
            cases.push(case(concat!($label, "-any-of"), untagged, invalid(false, $key)));
            for inline in [false, true] {
                cases.push(composed_case::<Tagged, $ty>(concat!($label, "-compose-one-of"), inline, tagged_json.clone(), invalid(true, $key)));
                cases.push(composed_case::<Untagged, $ty>(concat!($label, "-compose-any-of"), inline, untagged_json.clone(), invalid(false, $key)));
            }
            cases.push(case(concat!($label, "-optional-branch"), vec![
                OptionalBranch { id: "one".into(), branch: None::<Tagged>, values: map.clone() },
                OptionalBranch { id: "one".into(), branch: Some(Tagged::A { a: "yes".into() }), values: map.clone() },
            ], vec![json!({"id":"one","b":2}), json!({"id":2}), json!({"id":"one","kind":"A","a":2})]));
            cases.push(case(concat!($label, "-optional-any-of-branch"), vec![
                OptionalBranch { id: "one".into(), branch: None::<Untagged>, values: map.clone() },
                OptionalBranch { id: "one".into(), branch: Some(Untagged::A { a: "yes".into() }), values: map.clone() },
            ], vec![json!({"id":"one","b":2}), json!({"id":2}), json!({"id":"one","a":2})]));
            cases.push(case(concat!($label, "-optional-map"), vec![
                OptionalValues { id: "one".into(), branch: Tagged::A { a: "yes".into() }, values: None::<$ty> },
                OptionalValues { id: "one".into(), branch: Tagged::B { b: true }, values: Some(map.clone()) },
            ], invalid(true, $key)));
            cases.push(case(concat!($label, "-nested"), vec![Nested {
                note: "outer".into(), inner: Parent { id: "one".into(), branch: Tagged::A { a: "yes".into() }, values: map.clone() }
            }], vec![json!({"note":2,"id":"one","kind":"A","a":"yes"}), json!({"note":"outer","id":"one","kind":"A","a":"yes","b":2})]));
            let optional = OptionalMap { id: "one".into(), values: Some(map.clone()) };
            let encoded = serde_json::to_value(&optional).unwrap();
            let decoded: OptionalMap<$ty> = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
            cases.push(case(concat!($label, "-ordinary-roundtrip"), vec![OptionalMap::<$ty> { id: "one".into(), values: None }, optional], vec![json!({"id":2}), json!({"id":"one","rogue":2})]));
            cases.push(case(concat!($label, "-original-component"), vec![map], vec![json!({"id":2}), json!({"a":2}), json!({"kind":2})]));
        }};
    }
    add_map_cases!(
        EnumMap,
        EnumMap(BTreeMap::from([(MapKey::Extra, 2)])),
        Some("extra"),
        "enum"
    );
    add_map_cases!(
        PatternMap,
        PatternMap(BTreeMap::from([("x_ok".into(), 2)])),
        Some("x_ok"),
        "pattern"
    );
    add_map_cases!(ClosedMap, ClosedMap(BTreeMap::new()), None, "false");

    cases.push(case(
        "same-key-conditional-value",
        vec![
            Parent {
                id: "one".into(),
                branch: NumericBranch::A {
                    twelve: "fixed".into(),
                },
                values: BTreeMap::<u32, u32>::new(),
            },
            Parent {
                id: "one".into(),
                branch: NumericBranch::B { b: true },
                values: BTreeMap::from([(12, 3)]),
            },
        ],
        vec![
            json!({"id":"one","kind":"B","b":true,"12":"bad"}),
            json!({"id":"one","kind":"A","12":3}),
            json!({"id":"one","kind":"B","b":true,"rogue":3}),
            json!({"id":2,"kind":"B","b":true,"12":3}),
        ],
    ));

    // Successful anyOf branches contribute their property annotations. Failed
    // branches cannot exempt a field merely because its map value is valid.
    let fixed = json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]});
    let alternatives = json!([
        {"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"maxProperties":4},
        {"type":"object","properties":{"b":{"type":"boolean"}},"required":["b"]}
    ]);
    let map = json!({"type":"object","propertyNames":{"enum":["extra"]},"additionalProperties":{"type":"integer"}});
    cases.push(synthetic_case(
        "overlapping-any-of-annotations",
        vec![
            fixed.clone(),
            json!({"anyOf":alternatives.clone()}),
            map.clone(),
        ],
        vec![
            json!({"id":"one","a":"yes","extra":2}),
            json!({"id":"one","b":true,"extra":2}),
            json!({"id":"one","a":"yes","b":true,"extra":2}),
        ],
        vec![
            json!({"id":"one","a":"yes","b":2,"extra":2}),
            json!({"id":"one","a":2,"b":true,"extra":2}),
            json!({"id":"one","a":"yes","b":true,"extra":2,"another":3}),
            json!({"id":2,"a":"yes","extra":2}),
        ],
    ));
    cases.push(synthetic_case(
        "one-of-retains-exclusivity",
        vec![fixed.clone(), json!({"oneOf":alternatives}), map],
        vec![
            json!({"id":"one","a":"yes","extra":2}),
            json!({"id":"one","b":true,"extra":2}),
        ],
        vec![
            json!({"id":"one","a":"yes","b":true,"extra":2}),
            json!({"id":"one","extra":2}),
        ],
    ));
    // A map's own property still has to satisfy its propertyNames. A second
    // flattened child declaring the same name cannot waive that restriction.
    cases.push(synthetic_case(
        "own-property-name-is-not-an-external-exemption",
        vec![
            fixed,
            json!({"type":"object","properties":{"own":{"type":"string"}}}),
            json!({"type":"object","properties":{"own":{"type":"string"}},"propertyNames":{"enum":["extra"]},"additionalProperties":{"type":"integer"}}),
        ],
        vec![json!({"id":"one","extra":2})],
        vec![json!({"id":"one","own":"yes","extra":2}), json!({"id":"one","extra":"bad"})],
    ));
    cases.push(synthetic_case(
        "full-branch-guard-retains-max-properties",
        vec![
            json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
            json!({"anyOf":[
                {"type":"object","properties":{"a":{"type":"integer"}},"required":["a"],"maxProperties":3},
                {"type":"object","properties":{"b":{"type":"boolean"}},"required":["b"]}
            ]}),
            json!({"type":"object","propertyNames":{"enum":["extra"]},"additionalProperties":{"type":"integer"}}),
        ],
        vec![json!({"id":"one","a":2,"extra":3}), json!({"id":"one","b":true,"extra":3})],
        // The a branch fails maxProperties. Its integer-valued a cannot leak
        // into the successful b branch's map just because its value is valid.
        vec![json!({"id":"one","a":2,"b":true,"extra":3})],
    ));
    cases.push(synthetic_case(
        "unconditional-map-scopes-retain-both-name-rules",
        vec![
            json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
            json!({"anyOf":[
                {"type":"object","properties":{"a":{"type":"string"}},"required":["a"]},
                {"type":"object","properties":{"b":{"type":"boolean"}},"required":["b"]}
            ]}),
            json!({"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"propertyNames":{"enum":["label","extra","only_a"]},"additionalProperties":{"type":"integer"}}),
            json!({"type":"object","properties":{"enabled":{"type":"boolean"}},"required":["enabled"],"propertyNames":{"enum":["enabled","extra","only_b"]},"additionalProperties":{"type":"integer"}}),
        ],
        vec![json!({"id":"one","a":"yes","label":"map","enabled":true,"extra":2})],
        vec![
            json!({"id":"one","a":"yes","label":"map","enabled":true,"only_a":2}),
            json!({"id":"one","a":"yes","label":"map","enabled":true,"only_b":2}),
            json!({"id":"one","a":"yes","label":2,"enabled":true,"extra":2}),
            json!({"id":"one","a":"yes","label":"map","enabled":2,"extra":2}),
        ],
    ));

    cases.push(case(
        "nested-outer-name-matches-inactive-one-of-property",
        vec![NestedAlias {
            a: "outer".into(),
            inner: Parent {
                id: "one".into(),
                branch: Tagged::B { b: true },
                values: EnumMap(BTreeMap::from([(MapKey::Extra, 2)])),
            },
        }],
        vec![
            json!({"a":2,"id":"one","kind":"B","b":true,"extra":2}),
            json!({"a":"outer","id":"one","kind":"B","b":2,"extra":2}),
            json!({"a":"outer","id":"one","kind":"B","b":true,"rogue":2}),
        ],
    ));
    cases.push(case(
        "nested-outer-name-matches-inactive-any-of-property",
        vec![NestedAlias {
            a: "outer".into(),
            inner: Parent {
                id: "one".into(),
                branch: GuardedUntagged::B { b: true },
                values: EnumMap(BTreeMap::from([(MapKey::Extra, 2)])),
            },
        }],
        vec![
            json!({"a":2,"id":"one","b":true,"extra":2}),
            json!({"a":"outer","id":"one","b":true,"rogue":2}),
        ],
    ));

    let inner_owned_name = flatten_json(
        vec![
            json!({"type":"object","properties":{"own":{"type":"string"}},"propertyNames":{"enum":["extra"]},"additionalProperties":{"type":"integer"}}),
        ],
        &Components::new(),
    );
    cases.push(synthetic_case(
        "nested-own-name-policy-retains-original-owner",
        vec![
            json!({"type":"object","properties":{"own":{"type":"string"}}}),
            inner_owned_name,
        ],
        vec![json!({"extra":2})],
        vec![json!({"own":"yes","extra":2}), json!({"own":2,"extra":2})],
    ));

    for keyword in ["anyOf", "oneOf"] {
        let branches = json!([
            {"type":"object","properties":{"kind":{"const":"A"},"a":{"type":"string"}},"required":["kind","a"],"propertyNames":{"enum":["kind","a"]},"additionalProperties":false},
            {"type":"object","properties":{"kind":{"const":"B"},"b":{"type":"boolean"}},"required":["kind","b"],"propertyNames":{"enum":["kind","b"]},"additionalProperties":false}
        ]);
        let inner = flatten_json(
            vec![
                json!({(keyword):branches}),
                json!({"type":"object","propertyNames":{"enum":["extra"]},"additionalProperties":{"type":"integer"}}),
            ],
            &Components::new(),
        );
        cases.push(synthetic_case(
            &format!("nested-{keyword}-name-and-value-guards-receive-outer-fields"),
            vec![
                json!({"type":"object","properties":{"label":{"type":"string"}},"required":["label"]}),
                inner,
            ],
            vec![json!({"label":"outer","kind":"A","a":"yes"}), json!({"label":"outer","kind":"B","b":true})],
            vec![
                json!({"label":2,"kind":"A","a":"yes"}),
                json!({"label":"outer","kind":"A","a":"yes","b":2}),
                json!({"label":"outer","kind":"A","a":"yes","extra":2}),
            ],
        ));

        // The root policy owns the fields declared below its allOf, even when
        // another flattened sibling later declares the same name.
        let branches = json!([
            {"allOf":[{"type":"object","properties":{"own":{"type":"string"},"kind":{"const":"A"}},"required":["kind"]}],"propertyNames":{"enum":["kind","extra"]}},
            {"type":"object","properties":{"kind":{"const":"B"},"b":{"type":"boolean"}},"required":["kind","b"],"propertyNames":{"enum":["kind","b","extra"]}}
        ]);
        let inner = flatten_json(
            vec![
                json!({(keyword):branches}),
                json!({"type":"object","propertyNames":{"enum":["extra"]},"additionalProperties":{"type":"integer"}}),
            ],
            &Components::new(),
        );
        cases.push(synthetic_case(
            &format!("nested-{keyword}-root-policy-retains-owned-name"),
            vec![
                json!({"type":"object","properties":{"own":{"type":"string"}}}),
                inner,
            ],
            vec![
                json!({"kind":"A","extra":2}),
                json!({"own":"outer","kind":"B","b":true,"extra":2}),
            ],
            vec![
                json!({"own":"outer","kind":"A","extra":2}),
                json!({"kind":"A","extra":"bad"}),
            ],
        ));
    }

    let mut components = Components::new();
    components
        .schemas
        .insert("OpaqueAlias", Ref::new("#/$defs/Opaque"));
    let original = components.schemas.clone();
    let opaque_owner = json!({"allOf":[{"$ref":"#/components/schemas/OpaqueAlias"}],"propertyNames":{"enum":["extra"]}});
    let schema = flatten_json(
        vec![
            json!({"type":"object","properties":{"own":{"type":"string"}}}),
            opaque_owner,
        ],
        &components,
    );
    assert_eq!(components.schemas, original);
    cases.push(json!({
        "name":"local-alias-to-opaque-reference-retains-name-policy",
        "schema":{"allOf":[schema],"components":components,"$defs":{"Opaque":{"type":"object","properties":{"own":{"type":"string"}}}}},
        "valid":[{"extra":2}],
        "invalid":[{"own":"outer","extra":2},{"own":2,"extra":2}]
    }));

    for (name, guard) in [
        (
            "if-then",
            json!({"if":{"required":["a"]},"then":{"required":["permit"]}}),
        ),
        (
            "dependent-schemas",
            json!({"dependentSchemas":{"a":{"required":["permit"]}}}),
        ),
    ] {
        let mut branch = json!({"type":"object","properties":{"a":{"type":"integer"},"permit":{"type":"boolean"}},"required":["a"]});
        branch
            .as_object_mut()
            .unwrap()
            .extend(guard.as_object().unwrap().clone());
        cases.push(synthetic_case(
            &format!("conditional-key-projection-retains-{name}-guard"),
            vec![
                json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
                json!({"anyOf":[branch,{"type":"object","properties":{"b":{"type":"boolean"}},"required":["b"]}]}),
                json!({"type":"object","propertyNames":{"enum":["extra"]},"additionalProperties":{"type":"integer"}}),
            ],
            vec![json!({"id":"one","a":2,"permit":true,"extra":3}), json!({"id":"one","b":true,"extra":3})],
            vec![json!({"id":"one","a":2,"b":true,"extra":3})],
        ));
    }

    for target_has_catch_all in [false, true] {
        for policy_allows_own in [false, true] {
            let mut owner = Object::new()
                .property("own", Object::with_type(BasicType::Integer))
                .required("own");
            if target_has_catch_all {
                owner = owner.additional_properties(
                    salvo::oapi::schema::AdditionalProperties::FreeForm(true),
                );
            }
            let mut components = Components::new();
            components.schemas.insert("Owner", owner);
            let original = components.schemas.clone();
            let mut reference = Ref::from_schema_name("Owner");
            reference.extensions.insert(
                "propertyNames".into(),
                if policy_allows_own {
                    json!({"enum":["own","extra"]})
                } else {
                    json!({"enum":["extra"]})
                },
            );
            let schema = flatten_json(
                vec![
                    json!({"type":"object","properties":{"id":{"type":"string"},"own":{"type":"integer"}},"required":["id","own"]}),
                    json!({"anyOf":[reference]}),
                ],
                &components,
            );
            assert_eq!(components.schemas, original);
            let valid = if policy_allows_own {
                vec![json!({"id":"one","own":2,"extra":3})]
            } else {
                vec![]
            };
            let invalid = if policy_allows_own {
                vec![
                    json!({"id":2,"own":2,"extra":3}),
                    json!({"id":"one","own":"bad","extra":3}),
                    json!({"id":"one","own":2,"rogue":3}),
                ]
            } else {
                vec![json!({"id":"one","own":2,"extra":3})]
            };
            cases.push(json!({
                "name":format!("ref-policy-owner-allows-own-{policy_allows_own}-target-ap-{target_has_catch_all}"),
                "schema":{"allOf":[schema],"components":components},
                "valid":valid,"invalid":invalid
            }));
        }
    }

    let names: RefOr<Schema> = serde_json::from_value(json!({"enum":["kind","extra"]})).unwrap();
    let all_owner = Object::new()
        .property_names(names.clone())
        .add_extension("allOf", json!([{"properties":{"own":{"type":"integer"}}}]));
    let conditional_owner = Object::new()
        .property_names(names)
        .add_extension(
            "if",
            json!({"properties":{"kind":{"const":"A"}},"required":["kind"]}),
        )
        .add_extension(
            "then",
            json!({"properties":{"own":{"type":"integer"}},"required":["own"]}),
        );
    cases.push(synthetic_case(
        "typed-object-policy-retains-all-of-and-conditional-owners",
        vec![
            json!({"type":"object","properties":{"id":{"type":"string"},"own":{"type":"integer"}},"required":["id"]}),
            json!({"anyOf":[all_owner,conditional_owner]}),
        ],
        vec![json!({"id":"one","kind":"B","extra":3})],
        vec![json!({"id":"one","kind":"A","own":2,"extra":3}), json!({"id":2,"kind":"B","extra":3})],
    ));

    // A policy attached directly to the alias reference has the opaque target
    // as its owner too; resolving the alias must not detach that sibling.
    let mut components = Components::new();
    components
        .schemas
        .insert("OpaqueAlias", Ref::new("#/$defs/Opaque"));
    let original = components.schemas.clone();
    let mut opaque_reference = Ref::from_schema_name("OpaqueAlias");
    opaque_reference
        .extensions
        .insert("propertyNames".into(), json!({"enum":["extra"]}));
    let schema = flatten_json(
        vec![
            json!({"type":"object","properties":{"own":{"type":"string"}}}),
            json!(opaque_reference),
        ],
        &components,
    );
    assert_eq!(components.schemas, original);
    cases.push(json!({
        "name":"direct-ref-sibling-policy-retains-opaque-alias-owner",
        "schema":{"allOf":[schema],"components":components,"$defs":{"Opaque":{"type":"object","properties":{"own":{"type":"string"}}}}},
        "valid":[{"extra":2}],
        "invalid":[{"own":"owned by opaque target"},{"own":"owned by opaque target","extra":2}]
    }));

    if let Some(path) = std::env::var_os("SALVO_SCHEMA_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}
