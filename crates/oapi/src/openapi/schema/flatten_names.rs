//! Keep flattened map name constraints separate from property-value annotations.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::{Value, json};

use super::{AllOf, Object, SchemaType};
use crate::{Components, PropMap, RefOr, Schema};

pub(super) struct NamePolicy {
    pub rule: Value,
    pub own: BTreeSet<String>,
}

fn either(left: Value, right: Value) -> Value {
    if left == right || right == Value::Bool(false) || left == Value::Bool(true) {
        left
    } else if left == Value::Bool(false) || right == Value::Bool(true) {
        right
    } else {
        json!({"anyOf": [left, right]})
    }
}

fn both(left: Value, right: Value) -> Value {
    if left == Value::Bool(true) || left == right {
        right
    } else if right == Value::Bool(true) {
        left
    } else {
        json!({"allOf": [left, right]})
    }
}

fn merge(output: &mut BTreeMap<String, Value>, source: BTreeMap<String, Value>, condition: Value) {
    for (name, guard) in source {
        let guard = both(condition.clone(), guard);
        let previous = output.remove(&name).unwrap_or(Value::Bool(false));
        output.insert(name, either(previous, guard));
    }
}

// A guard validates the whole object exactly when a successful in-place
// subschema declares this property. Never descend into property values/items.
fn guards(
    schema: &Value,
    components: &Components,
    visiting: &mut HashSet<String>,
) -> BTreeMap<String, Value> {
    let mut output = BTreeMap::new();
    let Some(schema) = schema.as_object() else {
        return output;
    };
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        output.extend(
            properties
                .keys()
                .map(|name| (name.clone(), Value::Bool(true))),
        );
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && visiting.insert(reference.to_owned())
    {
        if let Some(target) = reference
            .strip_prefix("#/components/schemas/")
            .and_then(|name| {
                components
                    .schemas
                    .get(&name.replace("~1", "/").replace("~0", "~"))
            })
        {
            let target = serde_json::to_value(target).expect("schema serializes to JSON");
            merge(
                &mut output,
                guards(&target, components, visiting),
                Value::Bool(true),
            );
        }
        visiting.remove(reference);
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(items) = schema.get(keyword).and_then(Value::as_array) {
            for item in items {
                let condition = if keyword == "allOf" {
                    Value::Bool(true)
                } else {
                    item.clone()
                };
                merge(&mut output, guards(item, components, visiting), condition);
            }
        }
    }
    if let Some(condition) = schema.get("if") {
        merge(
            &mut output,
            guards(condition, components, visiting),
            condition.clone(),
        );
        if let Some(then) = schema.get("then") {
            merge(
                &mut output,
                guards(then, components, visiting),
                both(condition.clone(), then.clone()),
            );
        }
        if let Some(otherwise) = schema.get("else") {
            merge(
                &mut output,
                guards(otherwise, components, visiting),
                both(json!({"not": condition}), otherwise.clone()),
            );
        }
    }
    if let Some(dependencies) = schema.get("dependentSchemas").and_then(Value::as_object) {
        for (name, dependency) in dependencies {
            merge(
                &mut output,
                guards(dependency, components, visiting),
                both(json!({"required": [name]}), dependency.clone()),
            );
        }
    }
    output
}

pub(super) fn declared_names(schema: &Value, components: &Components) -> BTreeSet<String> {
    guards(schema, components, &mut HashSet::new())
        .into_keys()
        .collect()
}

pub(super) fn own_names(schema: &Value, components: &Components) -> Option<BTreeSet<String>> {
    fn complete(schema: &Value, components: &Components, visiting: &mut HashSet<String>) -> bool {
        let Some(schema) = schema.as_object() else {
            return true;
        };
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            if !visiting.insert(reference.to_owned()) {
                return false;
            }
            let resolved = reference
                .strip_prefix("#/components/schemas/")
                .and_then(|name| {
                    components
                        .schemas
                        .get(&name.replace("~1", "/").replace("~0", "~"))
                })
                .is_some_and(|target| {
                    complete(
                        &serde_json::to_value(target).expect("schema serializes to JSON"),
                        components,
                        visiting,
                    )
                });
            visiting.remove(reference);
            if !resolved {
                return false;
            }
        }
        for keyword in ["allOf", "anyOf", "oneOf"] {
            if let Some(items) = schema.get(keyword).and_then(Value::as_array)
                && !items
                    .iter()
                    .all(|item| complete(item, components, visiting))
            {
                return false;
            }
        }
        for keyword in ["if", "then", "else", "not"] {
            if let Some(child) = schema.get(keyword)
                && !complete(child, components, visiting)
            {
                return false;
            }
        }
        schema
            .get("dependentSchemas")
            .and_then(Value::as_object)
            .is_none_or(|items| {
                items
                    .values()
                    .all(|item| complete(item, components, visiting))
            })
    }
    complete(schema, components, &mut HashSet::new()).then(|| declared_names(schema, components))
}

fn own_check(rule: &Value, own: &BTreeSet<String>) -> Value {
    json!({"not": {"propertyNames": {
        "anyOf": [rule, {"not": {"enum": own}}]
    }}})
}

pub(super) fn reference_check(
    rule: Value,
    own: Option<&BTreeSet<String>>,
    known: &BTreeSet<String>,
) -> RefOr<Schema> {
    let mut check = Object::with_type(SchemaType::any());
    if let Some(own) = own {
        let exempt = known.difference(own).collect::<Vec<_>>();
        let names = if exempt.is_empty() {
            rule.clone()
        } else {
            json!({"anyOf": [&rule, {"enum": exempt}]})
        };
        check = check.add_extension("propertyNames", names);
        if !own.is_empty() {
            check = check.add_extension("not", own_check(&rule, own));
        }
    } else {
        // An opaque owner must keep its original assertion. Do not interpret
        // an unresolved property set as an empty property set.
        check = check.add_extension("not", json!({"not": {"propertyNames": rule}}));
    }
    check.into()
}

// A composed child may already contain conditional name checks from an inner
// flatten. Its new parent's fixed fields are claimed before that child/map.
pub(super) fn exempt_known(
    extensions: &mut PropMap<String, Value>,
    known: &BTreeSet<String>,
    components: &Components,
) {
    fn prepare(schema: &mut Value, known: &BTreeSet<String>, components: &Components) {
        let own = own_names(schema, components);
        let Some(schema) = schema.as_object_mut() else {
            return;
        };
        let exempt = own
            .map(|own| known.difference(&own).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if !exempt.is_empty()
            && let Some(rule) = schema.get_mut("propertyNames")
        {
            *rule = json!({"anyOf": [rule.clone(), {"enum": exempt}]});
        }
        for keyword in ["allOf", "anyOf", "oneOf"] {
            if let Some(items) = schema.get_mut(keyword).and_then(Value::as_array_mut) {
                for item in items {
                    prepare(item, known, components);
                }
            }
        }
        for keyword in ["then", "else"] {
            if let Some(branch) = schema.get_mut(keyword) {
                prepare(branch, known, components);
            }
        }
        // These full branch guards must see the same parent-field exemptions
        // as the structural branch they mirror. General not assertions remain
        // untouched, including the immutable own-name check below.
        if let Some(guard) = schema
            .get_mut("if")
            .and_then(Value::as_object_mut)
            .and_then(|condition| condition.get_mut("not"))
            && own_names(guard, components).is_some()
            && let Ok(typed) = serde_json::from_value::<RefOr<Schema>>(guard.clone())
        {
            *guard = serde_json::to_value(super::flatten_with_known_properties(
                typed, known, components,
            ))
            .expect("schema serializes to JSON");
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(items) = extensions.get_mut(keyword).and_then(Value::as_array_mut) {
            for item in items {
                prepare(item, known, components);
            }
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = extensions.get_mut(keyword) {
            prepare(branch, known, components);
        }
    }
    if let Some(guard) = extensions
        .get_mut("if")
        .and_then(Value::as_object_mut)
        .and_then(|condition| condition.get_mut("not"))
        && own_names(guard, components).is_some()
        && let Ok(typed) = serde_json::from_value::<RefOr<Schema>>(guard.clone())
    {
        *guard = serde_json::to_value(super::flatten_with_known_properties(
            typed, known, components,
        ))
        .expect("schema serializes to JSON");
    }
}

pub(super) fn apply(schema: &mut AllOf, policies: Vec<NamePolicy>, components: &Components) {
    if policies.is_empty() {
        return;
    }
    let structural = serde_json::to_value(&*schema).expect("schema serializes to JSON");
    let guards = guards(&structural, components, &mut HashSet::new());
    for policy in policies {
        // A policy still constrains its own declared fields. Only other
        // flattened siblings can claim a name on behalf of this map.
        let siblings = guards
            .iter()
            .filter(|(name, _)| !policy.own.contains(*name));
        let mut names = Vec::new();
        let mut conditions = Vec::new();
        for (name, guard) in siblings {
            names.push(name);
            if guard != &Value::Bool(true) {
                conditions.push(json!({
                    // not discards the guard's evaluated-property annotations.
                    // Letting them escape here could bypass the outer value rule.
                    "if": {"not": guard},
                    "then": {"propertyNames": {
                        "anyOf": [&policy.rule, {"not": {"const": name}}]
                    }}
                }));
            }
        }
        let rule = if names.is_empty() {
            policy.rule.clone()
        } else {
            json!({"anyOf": [&policy.rule, {"enum": names}]})
        };
        let mut check = Object::with_type(SchemaType::any()).add_extension("propertyNames", rule);
        if !conditions.is_empty() {
            check = check.add_extension("allOf", Value::Array(conditions));
        }
        if !policy.own.is_empty() {
            // Preserve ownership across another flatten without annotating
            // inactive properties or exposing an exemption to a later parent.
            check = check.add_extension("not", own_check(&policy.rule, &policy.own));
        }
        schema.items.push(check.into());
    }
}
