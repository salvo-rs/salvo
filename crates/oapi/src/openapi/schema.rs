//! Implements [OpenAPI Schema Object][schema] types which can be
//! used to define field properties, enum values, array or object types.
//!
//! [schema]: https://spec.openapis.org/oas/latest.html#schema-object
mod all_of;
mod any_of;
mod array;
mod flatten_names;
mod number;
mod object;
mod one_of;

use std::ops::{Deref, DerefMut};

pub use all_of::AllOf;
pub use any_of::AnyOf;
pub use array::{Array, ArrayItems};
pub use number::Number;
pub use object::Object;
pub use one_of::OneOf;
use serde::{Deserialize, Serialize};

use crate::{PropMap, RefOr};

/// Describe an optional flattened field as either a valid child or an absent
/// property group. Used by generated schemas.
#[doc(hidden)]
pub fn optional_flatten_schema(
    schema: RefOr<Schema>,
    parent: &Object,
    components: &crate::Components,
) -> RefOr<Schema> {
    fn names(
        schema: &RefOr<Schema>,
        components: &crate::Components,
        seen: &mut std::collections::HashSet<String>,
        output: &mut std::collections::BTreeSet<String>,
    ) {
        match schema {
            RefOr::Ref(reference) => {
                if seen.insert(reference.ref_location.clone())
                    && let Some(schema) = reference
                        .ref_location
                        .strip_prefix("#/components/schemas/")
                        .and_then(|name| {
                            components
                                .schemas
                                .get(&name.replace("~1", "/").replace("~0", "~"))
                        })
                {
                    names(schema, components, seen, output);
                }
            }
            RefOr::Type(Schema::Object(object)) => output.extend(object.properties.keys().cloned()),
            RefOr::Type(Schema::AllOf(all)) => {
                for item in &all.items {
                    names(item, components, seen, output);
                }
            }
            RefOr::Type(Schema::AnyOf(any)) => {
                for item in &any.items {
                    names(item, components, seen, output);
                }
            }
            RefOr::Type(Schema::OneOf(one)) => {
                for item in &one.items {
                    names(item, components, seen, output);
                }
            }
            _ => {}
        }
    }
    let mut fields = std::collections::BTreeSet::new();
    names(
        &schema,
        components,
        &mut std::collections::HashSet::new(),
        &mut fields,
    );
    let known = parent.properties.keys().cloned().collect();
    let schema = flatten_with_known_properties(schema, &known, components);
    if fields.is_empty() {
        // Unknown/custom schemas retain their constraints; do not introduce a
        // blanket permissive branch when no property group can be identified.
        return schema;
    }
    fields.retain(|field| !parent.properties.contains_key(field));
    let absent = if fields.is_empty() {
        Object::new()
    } else {
        let any_present = fields
            .into_iter()
            .map(|field| serde_json::json!({"required":[field]}))
            .collect::<Vec<_>>();
        Object::new().add_extension("not", serde_json::json!({"anyOf":any_present}))
    };
    AnyOf::new().item(schema).item(absent).into()
}

/// Convert a map key's value schema to its JSON object-name representation.
/// This is an implementation detail used by generated schemas.
#[doc(hidden)]
pub fn serialized_map_key_schema(
    mut key: RefOr<Schema>,
    components: &crate::Components,
) -> RefOr<Schema> {
    let mut seen = std::collections::HashSet::new();
    while let RefOr::Ref(reference) = &key {
        if !seen.insert(reference.ref_location.clone()) {
            return Object::with_type(BasicType::String).into();
        }
        let Some(schema) = reference
            .ref_location
            .strip_prefix("#/components/schemas/")
            .and_then(|name| components.schemas.get(name))
        else {
            return Object::with_type(BasicType::String).into();
        };
        key = schema.clone();
    }
    // Boolean schemas express unconditional acceptance/rejection; converting
    // them to a string schema would discard an explicit composition constraint.
    if matches!(key, RefOr::Type(Schema::Boolean(_))) {
        return key;
    }
    let RefOr::Type(Schema::Object(object)) = key else {
        return Object::with_type(BasicType::String).into();
    };
    if object.schema_type == SchemaType::Basic(BasicType::String) {
        return (*object).into();
    }
    let mut names = Object::with_type(BasicType::String);
    match object.schema_type {
        SchemaType::Basic(BasicType::Integer) => {
            let unsigned = object
                .minimum
                .as_ref()
                .is_some_and(|minimum| match minimum {
                    Number::Int(value) => *value >= 0,
                    Number::UInt(_) => true,
                    Number::Float(value) => *value >= 0.0,
                });
            names.pattern = Some(
                if unsigned {
                    "^(0|[1-9][0-9]*)$"
                } else {
                    "^-?(0|[1-9][0-9]*)$"
                }
                .into(),
            );
        }
        SchemaType::Basic(BasicType::Boolean) => {
            names.enum_values = Some(vec![serde_json::json!("true"), serde_json::json!("false")]);
        }
        SchemaType::Basic(BasicType::Number) => {
            names.pattern = Some(r"^-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?$".into());
        }
        _ => {}
    }
    if let Some(values) = object.enum_values {
        names.enum_values = Some(
            values
                .into_iter()
                .map(|value| serde_json::Value::String(value.to_string()))
                .collect(),
        );
    }
    names.into()
}

// A flattened child receives the parent's whole object, while serde has already
// claimed the parent's own fields before reading the child map. Exempt only
// those known keys from catch-all rules; their declared schemas still validate
// them. Resolve references locally so the standalone component stays unchanged.
pub(crate) fn flatten_with_known_properties(
    schema: RefOr<Schema>,
    known: &std::collections::BTreeSet<String>,
    components: &crate::Components,
) -> RefOr<Schema> {
    prepare_known_properties(schema, known, components, false)
}

/// Exempt an internally injected tag from the payload's object rules.
/// Unlike ordinary serde flattening, this adds a property to the serialized
/// payload, so its property-count bounds must grow with that property.
#[doc(hidden)]
pub fn tagged_payload_schema(
    schema: RefOr<Schema>,
    tag: &str,
    components: &crate::Components,
) -> RefOr<Schema> {
    let original = serde_json::to_value(&schema).expect("schema serializes to JSON");
    let already_counted = flatten_names::own_names(&original, components)
        .is_some_and(|own| own.contains(tag))
        || original
            .get("required")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|required| required.iter().any(|name| name == tag));
    prepare_known_properties(
        schema,
        &std::collections::BTreeSet::from([tag.to_owned()]),
        components,
        !already_counted,
    )
}

fn prepare_known_properties(
    schema: RefOr<Schema>,
    known: &std::collections::BTreeSet<String>,
    components: &crate::Components,
    adjust_counts: bool,
) -> RefOr<Schema> {
    fn count_extensions(extensions: &mut PropMap<String, serde_json::Value>, delta: usize) {
        for keyword in ["minProperties", "maxProperties"] {
            if let Some(bound) = extensions.get_mut(keyword)
                && let Some(value) = bound.as_u64()
            {
                *bound = value.saturating_add(delta as u64).into();
            }
        }
    }

    fn extensions(
        extensions: &mut PropMap<String, serde_json::Value>,
        known: &std::collections::BTreeSet<String>,
        inject_properties: bool,
        own: Option<&std::collections::BTreeSet<String>>,
        components: &crate::Components,
        adjust_counts: bool,
    ) {
        flatten_names::exempt_known(extensions, known, components);
        let exempt = own
            .map(|own| known.difference(own).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if adjust_counts {
            count_extensions(extensions, exempt.len());
        }
        if inject_properties
            && (extensions.contains_key("additionalProperties")
                || extensions.contains_key("unevaluatedProperties"))
        {
            let properties = extensions
                .entry("properties".into())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(properties) = properties.as_object_mut() {
                for name in known {
                    properties
                        .entry(name.clone())
                        .or_insert(serde_json::Value::Bool(true));
                }
            }
        }
        if !exempt.is_empty()
            && let Some(names) = extensions.get_mut("propertyNames")
        {
            *names = serde_json::json!({"anyOf":[names.clone(), {"enum":exempt}]});
        }
    }

    fn prepare(
        schema: RefOr<Schema>,
        known: &std::collections::BTreeSet<String>,
        components: &crate::Components,
        visiting: &mut std::collections::HashSet<String>,
        adjust_counts: bool,
    ) -> RefOr<Schema> {
        match schema {
            RefOr::Ref(reference) => {
                if !visiting.insert(reference.ref_location.clone()) {
                    return reference.into();
                }
                let target = reference
                    .ref_location
                    .strip_prefix("#/components/schemas/")
                    .and_then(|name| {
                        components
                            .schemas
                            .get(&name.replace("~1", "/").replace("~0", "~"))
                    });
                let prepared = target.map(|target| {
                    prepare(target.clone(), known, components, visiting, adjust_counts)
                });
                visiting.remove(&reference.ref_location);
                let name_rule = reference.extensions.get("propertyNames");
                let name_own = name_rule.and_then(|_| {
                    let original =
                        serde_json::to_value(&reference).expect("schema serializes to JSON");
                    flatten_names::own_names(&original, components)
                });
                let prepare_names = name_own
                    .as_ref()
                    .is_some_and(|own| known.difference(own).next().is_some());
                let prepare_counts = adjust_counts
                    && ["minProperties", "maxProperties"]
                        .iter()
                        .any(|keyword| reference.extensions.contains_key(*keyword));
                match (target, prepared) {
                    (Some(target), Some(prepared))
                        if &prepared != target || prepare_names || prepare_counts =>
                    {
                        if reference.extensions.is_empty()
                            && reference.description.is_empty()
                            && reference.summary.is_empty()
                        {
                            return prepared;
                        }
                        let mut combined = AllOf::new().item(prepared);
                        if !reference.extensions.is_empty() {
                            let mut siblings = reference.extensions.clone();
                            siblings.remove("propertyNames");
                            if !siblings.is_empty() {
                                let value = serde_json::to_value(&siblings)
                                    .expect("reference keywords serialize to JSON");
                                let siblings = serde_json::from_value::<RefOr<Schema>>(value)
                                    .unwrap_or_else(|_| {
                                        let mut object = Object::with_type(SchemaType::any());
                                        object.extensions = siblings;
                                        object.into()
                                    });
                                combined = combined.item(prepare(
                                    siblings,
                                    known,
                                    components,
                                    visiting,
                                    adjust_counts,
                                ));
                            }
                            if let Some(rule) = name_rule {
                                combined = combined.item(flatten_names::reference_check(
                                    rule.clone(),
                                    name_own.as_ref(),
                                    known,
                                ));
                            }
                        }
                        if !reference.description.is_empty() {
                            combined.description = Some(reference.description);
                        }
                        if !reference.summary.is_empty() {
                            combined
                                .extensions
                                .insert("summary".into(), reference.summary.into());
                        }
                        combined.into()
                    }
                    _ => reference.into(),
                }
            }
            RefOr::Type(mut schema) => {
                let original = serde_json::to_value(&schema).expect("schema serializes to JSON");
                let own = flatten_names::own_names(&original, components);
                match &mut schema {
                    Schema::Object(object) => {
                        if adjust_counts {
                            let delta = own
                                .as_ref()
                                .map(|own| known.difference(own).count())
                                .unwrap_or(0);
                            object.min_properties = object
                                .min_properties
                                .map(|bound| bound.saturating_add(delta));
                            object.max_properties = object
                                .max_properties
                                .map(|bound| bound.saturating_add(delta));
                        }
                        let exempt = known
                            .iter()
                            .filter(|name| !object.properties.contains_key(*name))
                            .cloned()
                            .collect::<Vec<_>>();
                        if object.additional_properties.is_some()
                            || object.extensions.contains_key("additionalProperties")
                            || object.extensions.contains_key("unevaluatedProperties")
                        {
                            for name in &exempt {
                                object
                                    .properties
                                    .insert(name.clone(), Schema::Boolean(true).into());
                            }
                        }
                        let extension_names = exempt.iter().cloned().collect();
                        let name_exempt = own
                            .as_ref()
                            .map(|own| known.difference(own).cloned().collect::<Vec<_>>())
                            .unwrap_or_default();
                        if !name_exempt.is_empty()
                            && let Some(names) = object.property_names.take()
                        {
                            object.property_names = Some(Box::new(
                                AnyOf::new()
                                    .item(*names)
                                    .item(
                                        Object::with_type(BasicType::String)
                                            .enum_values(name_exempt),
                                    )
                                    .into(),
                            ));
                        }
                        // Object already has a typed properties map. A second
                        // one in flattened extensions would shadow its fields.
                        extensions(
                            &mut object.extensions,
                            &extension_names,
                            false,
                            own.as_ref(),
                            components,
                            adjust_counts,
                        );
                    }
                    Schema::AllOf(all) => {
                        all.items = std::mem::take(&mut all.items)
                            .into_iter()
                            .map(|item| prepare(item, known, components, visiting, adjust_counts))
                            .collect();
                        extensions(
                            &mut all.extensions,
                            known,
                            true,
                            own.as_ref(),
                            components,
                            adjust_counts,
                        );
                    }
                    Schema::AnyOf(any) => {
                        any.items = std::mem::take(&mut any.items)
                            .into_iter()
                            .map(|item| prepare(item, known, components, visiting, adjust_counts))
                            .collect();
                        extensions(
                            &mut any.extensions,
                            known,
                            true,
                            own.as_ref(),
                            components,
                            adjust_counts,
                        );
                    }
                    Schema::OneOf(one) => {
                        one.items = std::mem::take(&mut one.items)
                            .into_iter()
                            .map(|item| prepare(item, known, components, visiting, adjust_counts))
                            .collect();
                        extensions(
                            &mut one.extensions,
                            known,
                            true,
                            own.as_ref(),
                            components,
                            adjust_counts,
                        );
                    }
                    _ => {}
                }
                schema.into()
            }
        }
    }
    if known.is_empty() {
        schema
    } else {
        prepare(
            schema,
            known,
            components,
            &mut std::collections::HashSet::new(),
            adjust_counts,
        )
    }
}

/// Apply flattened catch-all rules after the known properties of every child.
/// Used by generated schemas when the concrete type of a flattened field may
/// only become known during generic schema composition.
#[doc(hidden)]
pub fn flatten_all_of_schema(schema: AllOf, components: &crate::Components) -> AllOf {
    fn referenced<'a>(
        reference: &crate::Ref,
        components: &'a crate::Components,
    ) -> Option<&'a RefOr<Schema>> {
        reference
            .ref_location
            .strip_prefix("#/components/schemas/")
            .and_then(|name| {
                components
                    .schemas
                    .get(&name.replace("~1", "/").replace("~0", "~"))
            })
    }

    fn extension_names(
        extensions: &PropMap<String, serde_json::Value>,
        names: &mut std::collections::BTreeSet<String>,
    ) {
        if let Some(properties) = extensions
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            names.extend(properties.keys().cloned());
        }
    }

    fn known_names(
        schema: &RefOr<Schema>,
        components: &crate::Components,
        visiting: &mut std::collections::HashSet<String>,
        names: &mut std::collections::BTreeSet<String>,
    ) {
        match schema {
            RefOr::Ref(reference) => {
                extension_names(&reference.extensions, names);
                if visiting.insert(reference.ref_location.clone()) {
                    if let Some(target) = referenced(reference, components) {
                        known_names(target, components, visiting, names);
                    }
                    visiting.remove(&reference.ref_location);
                }
            }
            RefOr::Type(Schema::Object(object)) => {
                names.extend(object.properties.keys().cloned());
                extension_names(&object.extensions, names);
            }
            RefOr::Type(Schema::AllOf(all)) => {
                extension_names(&all.extensions, names);
                for item in &all.items {
                    known_names(item, components, visiting, names);
                }
            }
            // Alternative-only fields must not exempt an inactive branch's
            // keys from a sibling map's value rules.
            RefOr::Type(Schema::AnyOf(any)) => extension_names(&any.extensions, names),
            RefOr::Type(Schema::OneOf(one)) => extension_names(&one.extensions, names),
            _ => {}
        }
    }

    fn take_extensions(
        extensions: &mut PropMap<String, serde_json::Value>,
        catch_all: &mut Vec<serde_json::Value>,
        name_policies: &mut Vec<flatten_names::NamePolicy>,
        own: Option<&std::collections::BTreeSet<String>>,
    ) {
        for keyword in ["additionalProperties", "unevaluatedProperties"] {
            if let Some(value) = extensions.remove(keyword) {
                catch_all.push(value);
            }
        }
        if let Some(own) = own
            && let Some(rule) = extensions.remove("propertyNames")
        {
            name_policies.push(flatten_names::NamePolicy {
                rule,
                own: own.clone(),
            });
        }
    }

    fn lift(
        schema: RefOr<Schema>,
        components: &crate::Components,
        visiting: &mut std::collections::HashSet<String>,
        catch_all: &mut Vec<serde_json::Value>,
        name_policies: &mut Vec<flatten_names::NamePolicy>,
    ) -> RefOr<Schema> {
        match schema {
            RefOr::Ref(mut reference) => {
                if !visiting.insert(reference.ref_location.clone()) {
                    return reference.into();
                }
                let target = referenced(&reference, components);
                if target.is_some() && reference.extensions.contains_key("propertyNames") {
                    let original =
                        serde_json::to_value(&reference).expect("schema serializes to JSON");
                    if let Some(own) = flatten_names::own_names(&original, components) {
                        let rule = reference
                            .extensions
                            .remove("propertyNames")
                            .expect("name policy exists");
                        name_policies.push(flatten_names::NamePolicy { rule, own });
                    } else {
                        // A local alias can still lead to an opaque owner.
                        // Do not later lift its sibling rule as an ownerless
                        // object and accidentally exempt that owner's fields.
                        visiting.remove(&reference.ref_location);
                        return reference.into();
                    }
                }
                let siblings = if reference.extensions.is_empty() {
                    None
                } else {
                    let value = serde_json::to_value(&reference.extensions)
                        .expect("schema serializes to JSON");
                    let Ok(siblings) = serde_json::from_value::<RefOr<Schema>>(value) else {
                        visiting.remove(&reference.ref_location);
                        return reference.into();
                    };
                    Some(siblings)
                };
                let normalized = target.map(|target| {
                    let target = if siblings.is_none()
                        && reference.description.is_empty()
                        && reference.summary.is_empty()
                    {
                        target.clone()
                    } else {
                        let mut annotated = AllOf::new().item(target.clone());
                        if let Some(siblings) = siblings {
                            annotated.items.push(siblings);
                        }
                        if !reference.description.is_empty() {
                            annotated.description = Some(reference.description.clone());
                        }
                        if !reference.summary.is_empty() {
                            annotated
                                .extensions
                                .insert("summary".into(), serde_json::json!(reference.summary));
                        }
                        annotated.into()
                    };
                    let normalized = lift(
                        target.clone(),
                        components,
                        visiting,
                        catch_all,
                        name_policies,
                    );
                    (target, normalized)
                });
                visiting.remove(&reference.ref_location);
                match normalized {
                    Some((target, normalized)) if normalized != target => normalized,
                    _ => reference.into(),
                }
            }
            RefOr::Type(mut schema) => {
                let has_names = match &schema {
                    Schema::Object(object) => {
                        object.property_names.is_some()
                            || object.extensions.contains_key("propertyNames")
                    }
                    Schema::AllOf(all) => all.extensions.contains_key("propertyNames"),
                    Schema::AnyOf(any) => any.extensions.contains_key("propertyNames"),
                    Schema::OneOf(one) => one.extensions.contains_key("propertyNames"),
                    _ => false,
                };
                let own = if has_names {
                    let original =
                        serde_json::to_value(&schema).expect("schema serializes to JSON");
                    flatten_names::own_names(&original, components)
                } else {
                    Some(std::collections::BTreeSet::new())
                };
                match &mut schema {
                    Schema::Object(object) => {
                        if let Some(value) = object.additional_properties.take() {
                            catch_all.push(
                                serde_json::to_value(value).expect("schema serializes to JSON"),
                            );
                        }
                        if let Some(own) = &own
                            && let Some(rule) = object.property_names.take()
                        {
                            name_policies.push(flatten_names::NamePolicy {
                                rule: serde_json::to_value(rule)
                                    .expect("schema serializes to JSON"),
                                own: own.clone(),
                            });
                        }
                        take_extensions(
                            &mut object.extensions,
                            catch_all,
                            name_policies,
                            own.as_ref(),
                        );
                    }
                    Schema::AllOf(all) => {
                        all.items = std::mem::take(&mut all.items)
                            .into_iter()
                            .map(|item| lift(item, components, visiting, catch_all, name_policies))
                            .collect();
                        take_extensions(
                            &mut all.extensions,
                            catch_all,
                            name_policies,
                            own.as_ref(),
                        );
                    }
                    // Only this node's rules are unconditional. A map inside
                    // an alternative must keep its catch-all conditional.
                    Schema::AnyOf(any) => {
                        take_extensions(
                            &mut any.extensions,
                            catch_all,
                            name_policies,
                            own.as_ref(),
                        );
                    }
                    Schema::OneOf(one) => {
                        take_extensions(
                            &mut one.extensions,
                            catch_all,
                            name_policies,
                            own.as_ref(),
                        );
                    }
                    _ => {}
                }
                schema.into()
            }
        }
    }

    let schema = RefOr::Type(Schema::AllOf(schema));
    let mut known = std::collections::BTreeSet::new();
    known_names(
        &schema,
        components,
        &mut std::collections::HashSet::new(),
        &mut known,
    );
    let mut catch_all = Vec::new();
    let mut name_policies = Vec::new();
    let schema = lift(
        schema,
        components,
        &mut std::collections::HashSet::new(),
        &mut catch_all,
        &mut name_policies,
    );
    // Capture each name policy's own fields before adding parent/sibling
    // placeholders. Conditional siblings are handled separately below.
    let schema = flatten_with_known_properties(schema, &known, components);
    let RefOr::Type(Schema::AllOf(mut schema)) = schema else {
        unreachable!("preparing an allOf preserves its outer composition");
    };
    if !catch_all.is_empty() {
        let value = if catch_all.len() == 1 {
            catch_all.pop().expect("one catch-all")
        } else {
            serde_json::json!({"allOf":catch_all})
        };
        schema
            .extensions
            .insert("unevaluatedProperties".into(), value);
    }
    flatten_names::apply(&mut schema, name_policies, components);
    schema
}

#[derive(Debug, Default)]
pub(crate) struct CompositionState {
    active: Vec<ActiveComposition>,
    completed: Vec<(CompositionKey, String)>,
    provisional: Vec<String>,
    transactions: usize,
}

impl CompositionState {
    pub(crate) fn clone_schemas(&self, schemas: &Schemas) -> Schemas {
        let mut schemas = schemas.clone();
        if !self.provisional.is_empty() {
            schemas.retain(|name, _| !self.provisional.contains(name));
        }
        schemas
    }
}

// Construction caches do not participate in the document's value equality.
impl PartialEq for CompositionState {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CompositionKey {
    declaration: std::any::TypeId,
    owner: &'static str,
    arguments: Vec<Option<serde_json::Value>>,
}

#[derive(Clone, Debug)]
struct ActiveComposition {
    key: CompositionKey,
    reference: Option<String>,
    checkpoint: usize,
}

/// Bound recursive generated composition to references owned by this document.
#[doc(hidden)]
pub fn compose_schema<T: ?Sized>(
    components: &mut crate::Components,
    declaration: std::any::TypeId,
    generics: &[Option<RefOr<Schema>>],
    build: impl FnOnce(&mut crate::Components) -> RefOr<Schema>,
) -> RefOr<Schema> {
    use std::hash::{Hash, Hasher};

    let key = CompositionKey {
        declaration,
        owner: std::any::type_name::<T>(),
        arguments: generics
            .iter()
            .map(|schema| {
                schema
                    .as_ref()
                    .map(|schema| serde_json::to_value(schema).expect("schema serializes to JSON"))
            })
            .collect(),
    };
    let schemas = &components.schemas;
    components
        .composition
        .completed
        .retain(|(_, name)| match schemas.get(name) {
            Some(RefOr::Ref(reference)) => reference
                .ref_location
                .strip_prefix("#/components/schemas/")
                .is_none_or(|target| {
                    schemas.contains_key(&target.replace("~1", "/").replace("~0", "~"))
                }),
            Some(_) => true,
            None => false,
        });
    if let Some((_, name)) = components
        .composition
        .completed
        .iter()
        .find(|(entry, _)| entry == &key)
    {
        return components
            .schemas
            .get(name)
            .expect("retained component exists")
            .clone();
    }
    if let Some(index) = components
        .composition
        .active
        .iter()
        .position(|entry| entry.key == key)
    {
        let reference = if let Some(name) = &components.composition.active[index].reference {
            name.clone()
        } else {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.declaration.hash(&mut hasher);
            key.owner.hash(&mut hasher);
            serde_json::to_string(&key.arguments)
                .expect("schema serializes to JSON")
                .hash(&mut hasher);
            let base = format!(
                "__salvo_compose_{}_{:016x}",
                crate::naming::response_component_name::<T>(),
                hasher.finish()
            );
            let mut name = base.clone();
            let mut counter = 1;
            while components.schemas.contains_key(&name)
                || components
                    .composition
                    .active
                    .iter()
                    .any(|entry| entry.reference.as_deref() == Some(&name))
            {
                counter += 1;
                name = format!("{base}_{counter}");
            }
            components
                .schemas
                .insert(name.clone(), Ref::from_schema_name(&name));
            components.composition.provisional.push(name.clone());
            components.composition.active[index].reference = Some(name.clone());
            name
        };
        return Ref::from_schema_name(reference).into();
    }
    components.composition.active.push(ActiveComposition {
        key: key.clone(),
        reference: None,
        checkpoint: components.composition.provisional.len(),
    });
    components.composition.transactions += 1;
    // A caller may catch a schema_with panic and reuse the document. Remove the
    // in-progress marker on unwind as well as on success, then preserve the panic.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(components)));
    let active = components
        .composition
        .active
        .iter()
        .rposition(|entry| entry.key == key)
        .map(|index| components.composition.active.remove(index));
    match result {
        Ok(schema) => {
            if let Some(name) = active.and_then(|entry| entry.reference) {
                components.schemas.insert(name.clone(), schema.clone());
                components.composition.completed.push((key, name));
            }
            components.composition.transactions -= 1;
            if components.composition.transactions == 0 {
                components.composition.provisional.clear();
            }
            schema
        }
        Err(error) => {
            if let Some(entry) = active {
                rollback_composition(components, entry.checkpoint);
            }
            components.composition.transactions =
                components.composition.transactions.saturating_sub(1);
            std::panic::resume_unwind(error)
        }
    }
}

fn rollback_composition(components: &mut crate::Components, checkpoint: usize) {
    let removed = components.composition.provisional.split_off(checkpoint);
    components.schemas.retain(|name, _| !removed.contains(name));
    components
        .composition
        .completed
        .retain(|(_, name)| !removed.contains(name));
    for entry in &mut components.composition.active {
        if entry
            .reference
            .as_ref()
            .is_some_and(|name| removed.contains(name))
        {
            entry.reference = None;
        }
    }
}

/// Register a named schema transactionally, keeping its recursive placeholder local.
#[doc(hidden)]
pub fn register_schema(
    components: &mut crate::Components,
    name: String,
    build: impl FnOnce(&mut crate::Components) -> RefOr<Schema>,
) {
    if components.schemas.contains_key(&name) {
        return;
    }
    let checkpoint = components.composition.provisional.len();
    components.composition.transactions += 1;
    components
        .schemas
        .insert(name.clone(), Ref::from_schema_name(&name));
    components.composition.provisional.push(name.clone());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(components)));
    components.composition.transactions = components.composition.transactions.saturating_sub(1);
    match result {
        Ok(schema) => {
            components.schemas.insert(name, schema);
            if components.composition.transactions == 0 {
                components.composition.provisional.clear();
            }
        }
        Err(error) => {
            rollback_composition(components, checkpoint);
            std::panic::resume_unwind(error)
        }
    }
}

/// Resolve generated generic overrides without guessing a type alias's order.
#[doc(hidden)]
pub fn resolve_compose_bindings(
    declaration: &str,
    caller_module: &str,
    source_path: &[&str],
    parameters: &[&str],
    arguments: Vec<Option<(&'static str, RefOr<Schema>)>>,
) -> Vec<Option<RefOr<Schema>>> {
    // Only module-path normalization is performed here. Concrete type names are
    // opaque equality keys, never parsed to reconstruct Rust generic arguments.
    let mut source = source_path;
    let declaration_matches = if source.first() == Some(&"") {
        declaration == source[1..].join("::")
    } else {
        let mut path: Vec<_> = caller_module.split("::").collect();
        if source.first() == Some(&"crate") {
            path.truncate(1);
            source = &source[1..];
        } else if source.first() == Some(&"self") {
            source = &source[1..];
        } else {
            while source.first() == Some(&"super") {
                path.pop();
                source = &source[1..];
            }
        }
        path.extend_from_slice(source);
        declaration == path.join("::") || declaration == source_path.join("::")
    };
    if declaration_matches
        && parameters.len() == arguments.len()
        && parameters
            .iter()
            .zip(&arguments)
            .all(|(expected, argument)| {
                argument
                    .as_ref()
                    .is_none_or(|(actual, _)| expected == actual)
            })
    {
        return arguments
            .into_iter()
            .map(|argument| argument.map(|(_, schema)| schema))
            .collect();
    }
    parameters
        .iter()
        .map(|parameter| {
            // A fixed alias argument may have the same concrete type as a varying
            // argument. Repeated actual parameters therefore cannot be mapped safely.
            if parameters.iter().filter(|name| *name == parameter).count() != 1 {
                return None;
            }
            let mut matches = arguments
                .iter()
                .flatten()
                .filter(|(name, _)| name == parameter);
            let (_, first) = matches.next()?;
            matches
                .all(|(_, schema)| schema == first)
                .then(|| first.clone())
        })
        .collect()
}

/// Apply a flattened map's key policy only to names not owned by fixed fields.
#[doc(hidden)]
pub fn with_flattened_map_key(
    mut object: Object,
    key: RefOr<Schema>,
    components: &crate::Components,
) -> Object {
    let key = serialized_map_key_schema(key, components);
    let key = if object.properties.is_empty() {
        key
    } else {
        AnyOf::new()
            .item(key)
            .item(
                Object::with_type(BasicType::String).enum_values(object.properties.keys().cloned()),
            )
            .into()
    };
    object.property_names = Some(Box::new(key));
    object
}

/// Attach annotations to a property schema without mutating a shared component.
#[doc(hidden)]
pub fn annotate_schema(mut schema: RefOr<Schema>, mut annotations: Object) -> RefOr<Schema> {
    macro_rules! common {
        ($target:expr) => {{
            if annotations.title.is_some() {
                $target.title = annotations.title.take();
            }
            if annotations.default_value.is_some() {
                $target.default_value = annotations.default_value.take();
            }
            if !annotations.examples.is_empty() {
                $target.examples = std::mem::take(&mut annotations.examples);
            }
        }};
    }
    macro_rules! optional {
        ($target:expr, $($field:ident),*) => {$(
            if annotations.$field.is_some() { $target.$field = annotations.$field.take(); }
        )*};
    }
    let extensions = match &mut schema {
        RefOr::Ref(reference) => &mut reference.extensions,
        RefOr::Type(Schema::Object(object)) => {
            common!(object);
            optional!(object, read_only, write_only, deprecated, xml);
            if !annotations.content_encoding.is_empty() {
                object.content_encoding = std::mem::take(&mut annotations.content_encoding);
            }
            if !annotations.content_media_type.is_empty() {
                object.content_media_type = std::mem::take(&mut annotations.content_media_type);
            }
            &mut object.extensions
        }
        RefOr::Type(Schema::Array(array)) => {
            common!(array);
            optional!(array, deprecated, xml);
            if !annotations.content_encoding.is_empty() {
                array.content_encoding = Some(std::mem::take(&mut annotations.content_encoding));
            }
            if !annotations.content_media_type.is_empty() {
                array.content_media_type =
                    Some(std::mem::take(&mut annotations.content_media_type));
            }
            &mut array.extensions
        }
        RefOr::Type(Schema::AllOf(all)) => {
            common!(all);
            &mut all.extensions
        }
        RefOr::Type(Schema::AnyOf(any)) => {
            common!(any);
            &mut any.extensions
        }
        RefOr::Type(Schema::OneOf(one)) => {
            common!(one);
            &mut one.extensions
        }
        RefOr::Type(Schema::Boolean(_)) => {
            return AllOf::new().item(schema).item(annotations).into();
        }
    };
    if let serde_json::Value::Object(values) =
        serde_json::to_value(annotations).expect("annotations serialize")
    {
        extensions.extend(values);
    }
    schema
}

/// Schemas collection for OpenApi.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Schemas(pub PropMap<String, RefOr<Schema>>);

impl<K, R> From<PropMap<K, R>> for Schemas
where
    K: Into<String>,
    R: Into<RefOr<Schema>>,
{
    fn from(inner: PropMap<K, R>) -> Self {
        Self(
            inner
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}
impl<K, R, const N: usize> From<[(K, R); N]> for Schemas
where
    K: Into<String>,
    R: Into<RefOr<Schema>>,
{
    fn from(inner: [(K, R); N]) -> Self {
        Self(
            <[(K, R)]>::into_vec(Box::new(inner))
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

impl Deref for Schemas {
    type Target = PropMap<String, RefOr<Schema>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Schemas {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Schemas {
    type Item = (String, RefOr<Schema>);
    type IntoIter = <PropMap<String, RefOr<Schema>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Schemas {
    /// Construct a new empty [`Schemas`]. This is effectively same as calling [`Schemas::default`].
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }
    /// Inserts a key-value pair into the instance and returns `self`.
    #[must_use]
    pub fn schema<K: Into<String>, V: Into<RefOr<Schema>>>(mut self, key: K, value: V) -> Self {
        self.insert(key, value);
        self
    }
    /// Inserts a key-value pair into the instance.
    pub fn insert<K: Into<String>, V: Into<RefOr<Schema>>>(&mut self, key: K, value: V) {
        self.0.insert(key.into(), value.into());
    }
    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Self) {
        let items = std::mem::take(&mut other.0);
        for item in items {
            self.insert(item.0, item.1);
        }
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I, K, V>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<RefOr<Schema>>,
    {
        for (k, v) in iter.into_iter() {
            self.insert(k, v);
        }
    }
}

/// Create an _`empty`_ [`Schema`] that serializes to _`null`_.
///
/// Can be used in places where an item can be serialized as `null`. This is used with unit type
/// enum variants and tuple unit types.
#[must_use]
pub fn empty() -> Schema {
    Schema::object(
        Object::new()
            .schema_type(SchemaType::AnyValue)
            .default_value(serde_json::Value::Null),
    )
}

/// Is super type for [OpenAPI Schema Object][schemas]. Schema is reusable resource what can be
/// referenced from path operations and other components using [`Ref`].
///
/// [schemas]: https://spec.openapis.org/oas/latest.html#schema-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged, rename_all = "camelCase")]
pub enum Schema {
    /// A boolean JSON Schema: `true` accepts every value and `false` rejects every value.
    ///
    /// Boolean schemas are supported by the OpenAPI 3.1 and 3.2 JSON Schema dialects.
    /// This differs from `{"type": "boolean"}`, which describes boolean instance values.
    Boolean(bool),

    // Try compositions before the permissive object fallback, including when a
    // composition declares an explicit `type` alongside its applicator.
    /// Creates a _OneOf_ type [composite Object][composite] schema. This schema
    /// is used to map multiple schemas together where API endpoint could return any of them.
    /// [`Schema::OneOf`] is created form complex enum where enum holds other than unit types.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    OneOf(OneOf),

    /// Creates an _AllOf_ type [composite Object][composite] schema.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    AllOf(AllOf),

    /// Creates a _AnyOf_ type [composite Object][composite] schema.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    AnyOf(AnyOf),

    /// Defines array schema from another schema. Typically used with
    /// [`Schema::Object`]. Slice and Vec types are translated to [`Schema::Array`] types.
    Array(Array),
    /// Defines object schema. Object is either `object` holding **properties** which are other
    /// [`Schema`]s or can be a field within the [`Object`].
    Object(Box<Object>),
}

impl Default for Schema {
    fn default() -> Self {
        Self::Object(Default::default())
    }
}

impl From<bool> for Schema {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<bool> for RefOr<Schema> {
    fn from(value: bool) -> Self {
        Self::Type(Schema::Boolean(value))
    }
}

impl Schema {
    /// Construct a new [`Schema`] object.
    #[must_use]
    pub fn object(obj: Object) -> Self {
        Self::Object(Box::new(obj))
    }
}

/// OpenAPI [Discriminator][discriminator] object which can be optionally used together with
/// [`OneOf`] composite object.
///
/// [discriminator]: https://spec.openapis.org/oas/latest.html#discriminator-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Discriminator {
    /// Defines a discriminator property name which must be found within all composite
    /// objects.
    pub property_name: String,

    /// An object to hold mappings between payload values and schema names or references.
    /// This field can only be populated manually. There is no macro support and no
    /// validation.
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub mapping: PropMap<String, String>,

    /// The schema name or URI reference to the schema that validates the payload when the
    /// discriminating property is absent, or holds a value with no explicit or implicit
    /// mapping. Added in OpenAPI 3.2.
    ///
    /// Required by the spec when the discriminating property is optional.
    ///
    /// See <https://spec.openapis.org/oas/v3.2.0.html#discriminator-object>.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_mapping: Option<String>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Discriminator {
    /// Construct a new [`Discriminator`] object with property name.
    ///
    /// # Examples
    ///
    /// Create a new [`Discriminator`] object for `pet_type` property.
    /// ```
    /// # use salvo_oapi::schema::Discriminator;
    /// let discriminator = Discriminator::new("pet_type");
    /// ```
    pub fn new<I: Into<String>>(property_name: I) -> Self {
        Self {
            property_name: property_name.into(),
            mapping: PropMap::new(),
            default_mapping: None,
            extensions: PropMap::new(),
        }
    }

    /// Add a mapping between a payload value and a schema name or URI reference.
    #[must_use]
    pub fn add_mapping<K: Into<String>, V: Into<String>>(mut self, value: K, schema: V) -> Self {
        self.mapping.insert(value.into(), schema.into());
        self
    }

    /// Set the fallback schema used when the discriminating property is missing or unmapped.
    /// Requires OpenAPI 3.2.
    #[must_use]
    pub fn default_mapping<I: Into<String>>(mut self, default_mapping: I) -> Self {
        self.default_mapping = Some(default_mapping.into());
        self
    }

    /// Add openapi extensions (`x-something`) for [`Discriminator`].
    #[must_use]
    pub fn extensions(mut self, extensions: PropMap<String, serde_json::Value>) -> Self {
        self.extensions = extensions;
        self
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// AdditionalProperties is used to define values of map fields of the [`Schema`].
///
/// The value can either be [`RefOr`] or _`bool`_.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum AdditionalProperties<T> {
    /// Use _`AdditionalProperties::FreeForm(true)`_ when any value is allowed in the map.
    FreeForm(bool),
    /// Use when value type of the map is a known [`Schema`] or [`Ref`] to the [`Schema`].
    RefOr(RefOr<T>),
}

impl<T> From<RefOr<T>> for AdditionalProperties<T> {
    fn from(value: RefOr<T>) -> Self {
        Self::RefOr(value)
    }
}

impl From<Object> for AdditionalProperties<Schema> {
    fn from(value: Object) -> Self {
        Self::RefOr(RefOr::Type(Schema::object(value)))
    }
}

impl From<Array> for AdditionalProperties<Schema> {
    fn from(value: Array) -> Self {
        Self::RefOr(RefOr::Type(Schema::Array(value)))
    }
}

impl From<Ref> for AdditionalProperties<Schema> {
    fn from(value: Ref) -> Self {
        Self::RefOr(RefOr::Ref(value))
    }
}

impl From<OneOf> for AdditionalProperties<Schema> {
    fn from(value: OneOf) -> Self {
        Self::RefOr(RefOr::Type(Schema::OneOf(value)))
    }
}

/// Implements [OpenAPI Reference Object][reference] that can be used to reference
/// reusable components such as [`Schema`]s or [`Response`](super::Response)s.
///
/// [reference]: https://spec.openapis.org/oas/latest.html#reference-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    /// Reference location of the actual component.
    #[serde(rename = "$ref")]
    pub ref_location: String,

    /// A description which by default should override that of the referenced component.
    /// Description supports markdown syntax. If referenced object type does not support
    /// description this field does not have effect.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub description: String,

    /// A short summary which by default should override that of the referenced component. If
    /// referenced component does not support summary field this does not have effect.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub summary: String,

    /// Additional fields alongside `$ref`, including Schema Object keywords.
    ///
    /// OpenAPI 3.1 Schema Objects can combine `$ref` with constraints such as
    /// `minLength` or `properties`. Preserve those fields when reading a schema.
    /// In other locations, OpenAPI Reference Object rules still determine which
    /// fields are meaningful; retaining a field does not apply schema semantics there.
    #[serde(default, skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Ref {
    /// Construct a new [`Ref`] with custom ref location. In most cases this is not necessary
    /// and [`Ref::from_schema_name`] could be used instead.
    #[must_use]
    pub fn new<I: Into<String>>(ref_location: I) -> Self {
        Self {
            ref_location: ref_location.into(),
            ..Default::default()
        }
    }

    /// Construct a new [`Ref`] from provided schema name. This will create a [`Ref`] that
    /// references the reusable schemas.
    #[must_use]
    pub fn from_schema_name<I: Into<String>>(schema_name: I) -> Self {
        Self::new(format!("#/components/schemas/{}", schema_name.into()))
    }

    /// Construct a new [`Ref`] from provided response name. This will create a [`Ref`] that
    /// references the reusable response.
    #[must_use]
    pub fn from_response_name<I: Into<String>>(response_name: I) -> Self {
        Self::new(format!("#/components/responses/{}", response_name.into()))
    }

    /// Add or change reference location of the actual component.
    #[must_use]
    pub fn ref_location(mut self, ref_location: String) -> Self {
        self.ref_location = ref_location;
        self
    }

    /// Add or change reference location of the actual component automatically formatting the $ref
    /// to `#/components/schemas/...` format.
    #[must_use]
    pub fn ref_location_from_schema_name<S: Into<String>>(mut self, schema_name: S) -> Self {
        self.ref_location = format!("#/components/schemas/{}", schema_name.into());
        self
    }

    // TODO: REMOVE THE unnecessary description Option wrapping.

    /// Add or change description which by default should override that of the referenced component.
    /// Description supports markdown syntax. If referenced object type does not support
    /// description this field does not have effect.
    #[must_use]
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = description.into();
        self
    }

    /// Add or change short summary which by default should override that of the referenced
    /// component. If referenced component does not support summary field this does not have
    /// effect.
    #[must_use]
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        self.summary = summary.into();
        self
    }

    /// Convert type to [`Array`].
    #[must_use]
    pub fn to_array(self) -> Array {
        Array::new().items(self)
    }
}

impl From<Ref> for RefOr<Schema> {
    fn from(r: Ref) -> Self {
        Self::Ref(r)
    }
}

impl<T> From<T> for RefOr<T> {
    fn from(t: T) -> Self {
        Self::Type(t)
    }
}

impl Default for RefOr<Schema> {
    fn default() -> Self {
        Self::Type(Schema::object(Object::new()))
    }
}

// impl ToArray for RefOr<Schema> {}

/// Represents type of [`Schema`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum SchemaType {
    /// Single type known from OpenAPI spec 3.0
    Basic(BasicType),
    /// Multiple types rendered as [`slice`]
    Array(Vec<BasicType>),
    /// Type that is considered typeless. _`AnyValue`_ will omit the type definition from the schema
    /// making it to accept any type possible.
    AnyValue,
}

impl Default for SchemaType {
    fn default() -> Self {
        Self::Basic(BasicType::default())
    }
}

impl From<BasicType> for SchemaType {
    fn from(value: BasicType) -> Self {
        Self::basic(value)
    }
}

impl FromIterator<BasicType> for SchemaType {
    fn from_iter<T: IntoIterator<Item = BasicType>>(iter: T) -> Self {
        Self::Array(iter.into_iter().collect())
    }
}
impl SchemaType {
    /// Instantiate new [`SchemaType`] of given [`BasicType`]
    ///
    /// Method accepts one argument `type` to create [`SchemaType`] for.
    ///
    /// # Examples
    ///
    /// _**Create string [`SchemaType`]**_
    /// ```rust
    /// # use salvo_oapi::schema::{SchemaType, BasicType};
    /// let ty = SchemaType::basic(BasicType::String);
    /// ```
    #[must_use]
    pub fn basic(r#type: BasicType) -> Self {
        Self::Basic(r#type)
    }

    //// Instantiate new [`SchemaType::AnyValue`].
    /// This is same as calling [`SchemaType::AnyValue`] but in a function form `() -> SchemaType`
    /// allowing it to be used as argument for _serde's_ _`default = "..."`_.
    #[must_use]
    pub fn any() -> Self {
        Self::AnyValue
    }

    /// Check whether this [`SchemaType`] is any value _(typeless)_ returning true on any value
    /// schema type.
    #[must_use]
    pub fn is_any_value(&self) -> bool {
        matches!(self, Self::AnyValue)
    }
}

/// Represents data type fragment of [`Schema`].
///
/// [`BasicType`] is used to create a [`SchemaType`] that defines the type of the [`Schema`].
/// [`SchemaType`] can be created from a single [`BasicType`] or multiple [`BasicType`]s according
/// to the OpenAPI 3.1 spec. Since the OpenAPI 3.1 is fully compatible with JSON schema the
/// definition of the _**type**_ property comes from [JSON Schema type](https://json-schema.org/understanding-json-schema/reference/type).
///
/// # Examples
/// _**Create nullable string [`SchemaType`]**_
/// ```rust
/// # use std::iter::FromIterator;
/// # use salvo_oapi::schema::{BasicType, SchemaType};
/// let _: SchemaType = [BasicType::String, BasicType::Null].into_iter().collect();
/// ```
/// _**Create string [`SchemaType`]**_
/// ```rust
/// # use salvo_oapi::schema::{BasicType, SchemaType};
/// let _ = SchemaType::basic(BasicType::String);
/// ```
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BasicType {
    /// Used with [`Object`] to describe schema that has _properties_ describing fields. have
    #[default]
    Object,
    /// Indicates string type of content. Used with [`Object`] on a `string`
    /// field.
    String,
    /// Indicates integer type of content. Used with [`Object`] on a `number`
    /// field.
    Integer,
    /// Indicates floating point number type of content. Used with
    /// [`Object`] on a `number` field.
    Number,
    /// Indicates boolean type of content. Used with [`Object`] on
    /// a `bool` field.
    Boolean,
    /// Used with [`Array`]. Indicates array type of content.
    Array,
    /// Null type. Used together with other type to indicate nullable values.
    Null,
}

/// Additional format for [`SchemaType`] to fine tune the data type used.
///
/// If the **format** is not supported by the UI it may default back to [`SchemaType`] alone.
/// Format is an open value, so you can use any formats, even not those defined by the
/// OpenAPI Specification.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase", untagged)]
pub enum SchemaFormat {
    /// Use to define additional detail about the value.
    KnownFormat(KnownFormat),
    /// Can be used to provide additional detail about the value when [`SchemaFormat::KnownFormat`]
    /// is not suitable.
    Custom(String),
}

/// Known schema format modifier property to provide fine detail of the primitive type.
///
/// Known format is defined in <https://spec.openapis.org/oas/latest.html#data-types> and
/// <https://datatracker.ietf.org/doc/html/draft-bhutton-json-schema-validation-00#section-7.3> as
/// well as by few known data types that are enabled by specific feature flag e.g. _`uuid`_.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KnownFormat {
    /// 8 bit integer.
    Int8,
    /// 16 bit integer.
    Int16,
    /// 32 bit integer.
    Int32,
    /// 64 bit integer.
    Int64,
    /// 8 bit unsigned integer.
    #[serde(rename = "uint8")]
    UInt8,
    /// 16 bit unsigned integer.
    #[serde(rename = "uint16")]
    UInt16,
    /// 32 bit unsigned integer.
    #[serde(rename = "uint32")]
    UInt32,
    /// 64 bit unsigned integer.
    #[serde(rename = "uint64")]
    UInt64,
    /// floating point number.
    Float,
    /// double (floating point) number.
    Double,
    /// base64 encoded chars.
    Byte,
    /// binary data (octet).
    Binary,
    /// ISO-8601 full time format [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    Time,
    /// ISO-8601 full date [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    Date,
    /// ISO-8601 full date time [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    DateTime,
    /// duration format from [RFC3339 Appendix-A](https://datatracker.ietf.org/doc/html/rfc3339#appendix-A).
    Duration,
    /// Hint to UI to obscure input.
    Password,
    /// Use for compact string
    String,
    /// Used with [`String`] values to indicate value is in decimal format.
    ///
    /// **decimal** feature need to be enabled.
    #[cfg(any(feature = "decimal", feature = "decimal-float"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "decimal", feature = "decimal-float"))))]
    Decimal,
    /// Used with [`String`] values to indicate value is in ULID format.
    #[cfg(feature = "ulid")]
    #[cfg_attr(docsrs, doc(cfg(feature = "ulid")))]
    Ulid,

    /// Used with [`String`] values to indicate value is in UUID format.
    #[cfg(feature = "uuid")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uuid")))]
    Uuid,
    /// Used with [`String`] values to indicate value is in Url format.
    ///
    /// **url** feature need to be enabled.
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    Url,
    /// A string instance is valid against this attribute if it is a valid URI Reference
    /// (either a URI or a relative-reference) according to
    /// [RFC3986](https://datatracker.ietf.org/doc/html/rfc3986).
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    UriReference,
    /// A string instance is valid against this attribute if it is a
    /// valid IRI, according to [RFC3987](https://datatracker.ietf.org/doc/html/rfc3987).
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    Iri,
    /// A string instance is valid against this attribute if it is a valid IRI Reference
    /// (either an IRI or a relative-reference)
    /// according to [RFC3987](https://datatracker.ietf.org/doc/html/rfc3987).
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    IriReference,
    /// As defined in "Mailbox" rule [RFC5321](https://datatracker.ietf.org/doc/html/rfc5321#section-4.1.2).
    Email,
    /// As defined by extended "Mailbox" rule [RFC6531](https://datatracker.ietf.org/doc/html/rfc6531#section-3.3).
    IdnEmail,
    /// As defined by [RFC1123](https://datatracker.ietf.org/doc/html/rfc1123#section-2.1), including host names
    /// produced using the Punycode algorithm
    /// specified in [RFC5891](https://datatracker.ietf.org/doc/html/rfc5891#section-4.4).
    Hostname,
    /// As defined by either [RFC1123](https://datatracker.ietf.org/doc/html/rfc1123#section-2.1) as for hostname,
    /// or an internationalized hostname as defined by [RFC5890](https://datatracker.ietf.org/doc/html/rfc5890#section-2.3.2.3).
    IdnHostname,
    /// An IPv4 address according to [RFC2673](https://datatracker.ietf.org/doc/html/rfc2673#section-3.2).
    Ipv4,
    /// An IPv6 address according to [RFC4291](https://datatracker.ietf.org/doc/html/rfc4291#section-2.2).
    Ipv6,
    /// A string instance is a valid URI Template if it is according to
    /// [RFC6570](https://datatracker.ietf.org/doc/html/rfc6570).
    ///
    /// _**Note!**_ There are no separate IRL template.
    UriTemplate,
    /// A valid JSON string representation of a JSON Pointer according to [RFC6901](https://datatracker.ietf.org/doc/html/rfc6901#section-5).
    JsonPointer,
    /// A valid relative JSON Pointer according to [draft-handrews-relative-json-pointer-01](https://datatracker.ietf.org/doc/html/draft-handrews-relative-json-pointer-01).
    RelativeJsonPointer,
    /// Regular expression, which SHOULD be valid according to the
    /// [ECMA-262](https://datatracker.ietf.org/doc/html/draft-bhutton-json-schema-validation-00#ref-ecma262).
    Regex,
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::{Value, json};

    use super::*;
    use crate::*;

    #[test]
    fn create_schema_serializes_json() -> Result<(), serde_json::Error> {
        let openapi = OpenApi::new("My api", "1.0.0").components(
            Components::new()
                .add_schema("Person", Ref::new("#/components/PersonModel"))
                .add_schema(
                    "Credential",
                    Schema::from(
                        Object::new()
                            .property(
                                "id",
                                Object::new()
                                    .schema_type(BasicType::Integer)
                                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                                    .description("Id of credential")
                                    .default_value(json!(1i32)),
                            )
                            .property(
                                "name",
                                Object::new()
                                    .schema_type(BasicType::String)
                                    .description("Name of credential"),
                            )
                            .property(
                                "status",
                                Object::new()
                                    .schema_type(BasicType::String)
                                    .default_value(json!("Active"))
                                    .description("Credential status")
                                    .enum_values(["Active", "NotActive", "Locked", "Expired"]),
                            )
                            .property(
                                "history",
                                Array::new().items(Ref::from_schema_name("UpdateHistory")),
                            )
                            .property("tags", Object::with_type(BasicType::String).to_array()),
                    ),
                ),
        );

        let serialized = serde_json::to_string_pretty(&openapi)?;
        println!("serialized json:\n {serialized}");

        let value = serde_json::to_value(&openapi)?;
        let credential = get_json_path(&value, "components.schemas.Credential.properties");
        let person = get_json_path(&value, "components.schemas.Person");

        assert!(
            credential.get("id").is_some(),
            "could not find path: components.schemas.Credential.properties.id"
        );
        assert!(
            credential.get("status").is_some(),
            "could not find path: components.schemas.Credential.properties.status"
        );
        assert!(
            credential.get("name").is_some(),
            "could not find path: components.schemas.Credential.properties.name"
        );
        assert!(
            credential.get("history").is_some(),
            "could not find path: components.schemas.Credential.properties.history"
        );
        assert_json_eq!(
            credential
                .get("id")
                .unwrap_or(&serde_json::value::Value::Null),
            json!({"type":"integer","format":"int32","description":"Id of credential","default":1})
        );
        assert_json_eq!(
            credential
                .get("name")
                .unwrap_or(&serde_json::value::Value::Null),
            json!({"type":"string","description":"Name of credential"})
        );
        assert_json_eq!(
            credential
                .get("status")
                .unwrap_or(&serde_json::value::Value::Null),
            json!({"default":"Active","description":"Credential status","enum":["Active","NotActive","Locked","Expired"],"type":"string"})
        );
        assert_json_eq!(
            credential
                .get("history")
                .unwrap_or(&serde_json::value::Value::Null),
            json!({"items":{"$ref":"#/components/schemas/UpdateHistory"},"type":"array"})
        );
        assert_eq!(person, &json!({"$ref":"#/components/PersonModel"}));

        Ok(())
    }

    // Examples taken from https://spec.openapis.org/oas/latest.html#model-with-map-dictionary-properties
    #[test]
    fn test_property_order() {
        let json_value = Object::new()
            .property(
                "id",
                Object::new()
                    .schema_type(BasicType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            )
            .property(
                "name",
                Object::new()
                    .schema_type(BasicType::String)
                    .description("Name of credential"),
            )
            .property(
                "status",
                Object::new()
                    .schema_type(BasicType::String)
                    .default_value(json!("Active"))
                    .description("Credential status")
                    .enum_values(["Active", "NotActive", "Locked", "Expired"]),
            )
            .property(
                "history",
                Array::new().items(Ref::from_schema_name("UpdateHistory")),
            )
            .property("tags", Object::with_type(BasicType::String).to_array());

        #[cfg(not(feature = "preserve-order"))]
        assert_eq!(
            json_value.properties.keys().collect::<Vec<_>>(),
            vec!["history", "id", "name", "status", "tags"]
        );

        #[cfg(feature = "preserve-order")]
        assert_eq!(
            json_value.properties.keys().collect::<Vec<_>>(),
            vec!["id", "name", "status", "history", "tags"]
        );
    }

    // Examples taken from https://spec.openapis.org/oas/latest.html#model-with-map-dictionary-properties
    #[test]
    fn test_additional_properties() {
        let json_value =
            Object::new().additional_properties(Object::new().schema_type(BasicType::String));
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "type": "string"
                }
            })
        );

        let json_value = Object::new().additional_properties(
            Array::new().items(Object::new().schema_type(BasicType::Number)),
        );
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "items": {
                        "type": "number",
                    },
                    "type": "array",
                }
            })
        );

        let json_value = Object::new().additional_properties(Ref::from_schema_name("ComplexModel"));
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "$ref": "#/components/schemas/ComplexModel"
                }
            })
        )
    }

    #[test]
    fn test_object_with_name() {
        let json_value = Object::new().name("SomeName");
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "name": "SomeName"
            })
        );
    }

    #[test]
    fn test_derive_object_with_examples() {
        let expected = r#"{"type":"object","examples":[{"age":20,"name":"bob the cat"}]}"#;
        let json_value = Object::new().examples([json!({"age": 20, "name": "bob the cat"})]);

        let value_string = serde_json::to_string(&json_value).unwrap();
        assert_eq!(
            value_string, expected,
            "value string != expected string, {value_string} != {expected}"
        );
    }

    fn get_json_path<'a>(value: &'a Value, path: &str) -> &'a Value {
        path.split('.').fold(value, |acc, fragment| {
            acc.get(fragment).unwrap_or(&serde_json::value::Value::Null)
        })
    }

    #[test]
    fn test_array_new() {
        let array = Array::new().items(
            Object::new().property(
                "id",
                Object::new()
                    .schema_type(BasicType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            ),
        );

        assert!(matches!(
            array.schema_type,
            SchemaType::Basic(BasicType::Array)
        ));
    }

    #[test]
    fn test_array_builder() {
        let array: Array = Array::new().items(
            Object::new().property(
                "id",
                Object::new()
                    .schema_type(BasicType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            ),
        );

        assert!(matches!(
            array.schema_type,
            SchemaType::Basic(BasicType::Array)
        ));
    }

    #[test]
    fn reserialize_deserialized_schema_components() {
        let components = Components::new()
            .extend_schemas(vec![(
                "Comp",
                Schema::from(
                    Object::new()
                        .property("name", Object::new().schema_type(BasicType::String))
                        .required("name"),
                ),
            )])
            .response("204", Response::new("No Content"))
            .extend_responses(vec![("200", Response::new("Okay"))])
            .add_security_scheme(
                "TLS",
                SecurityScheme::MutualTls {
                    description: None,
                    deprecated: None,
                },
            )
            .extend_security_schemes(vec![(
                "APIKey",
                SecurityScheme::Http(security::Http::default()),
            )]);

        let serialized_components = serde_json::to_string(&components).unwrap();

        let deserialized_components: Components =
            serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_object_component() {
        let prop = Object::new()
            .property("name", Object::new().schema_type(BasicType::String))
            .required("name");

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object =
            serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_property() {
        let prop = Object::new().schema_type(BasicType::String);

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object =
            serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn serialize_deserialize_array_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::Type(Schema::object(Object::new().property(
            "test",
            RefOr::Type(Schema::Array(Array::new().items(RefOr::Type(
                Schema::object(Object::new().property("element", RefOr::Ref(Ref::new("#/test")))),
            )))),
        )));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_one_of_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::Type(Schema::object(
            Object::new().property(
                "test",
                RefOr::Type(Schema::OneOf(
                    OneOf::new()
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::object(
                                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                            ),
                        ))))
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::object(
                                Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                            ),
                        )))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_all_of_of_within_ref_or_t_object() {
        let ref_or_schema = RefOr::Type(Schema::object(
            Object::new().property(
                "test",
                RefOr::Type(Schema::AllOf(
                    AllOf::new()
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::object(
                                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                            ),
                        ))))
                        .item(RefOr::Type(Schema::object(
                            Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                        ))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_any_of_of_within_ref_or_t_object() {
        let ref_or_schema = RefOr::Type(Schema::object(
            Object::new().property(
                "test",
                RefOr::Type(Schema::AnyOf(
                    AnyOf::new()
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::object(
                                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                            ),
                        ))))
                        .item(RefOr::Type(Schema::object(
                            Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                        ))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert!(json_str.contains("\"anyOf\""));
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array_ref_or_t() {
        let ref_or_schema = RefOr::Type(Schema::Array(Array::new().items(RefOr::Type(
            Schema::Object(Box::new(
                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
            )),
        ))));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array() {
        let ref_or_schema = Array::new().items(RefOr::Type(Schema::object(
            Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
        )));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties() {
        let schema = Schema::object(Object::new().property(
            "map",
            Object::new().additional_properties(AdditionalProperties::FreeForm(true)),
        ));

        let json_str = serde_json::to_string(&schema).unwrap();
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).unwrap();
        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties_object() {
        let schema = Schema::object(Object::new().property(
            "map",
            Object::new().additional_properties(
                Object::new().property("name", Object::with_type(BasicType::String)),
            ),
        ));

        let json_str = serde_json::to_string(&schema).expect("serde json should success");
        let deserialized: RefOr<Schema> =
            serde_json::from_str(&json_str).expect("serde json should success");
        let json_de_str = serde_json::to_string(&deserialized).expect("serde json should success");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_discriminator_with_mapping() {
        let mut discriminator = Discriminator::new("type");
        discriminator.mapping = [("int".to_owned(), "#/components/schemas/MyInt".to_owned())]
            .into_iter()
            .collect::<PropMap<_, _>>();
        let one_of = OneOf::new()
            .item(Ref::from_schema_name("MyInt"))
            .discriminator(discriminator);
        let json_value = serde_json::to_value(one_of).expect("serde json should success");

        assert_json_eq!(
            json_value,
            json!({
                "oneOf": [
                    {
                        "$ref": "#/components/schemas/MyInt"
                    }
                ],
                "discriminator": {
                    "propertyName": "type",
                    "mapping": {
                        "int": "#/components/schemas/MyInt"
                    }
                }
            })
        );
    }

    #[test]
    fn deserialize_reserialize_one_of_default_type() {
        let a = OneOf::new()
            .item(Schema::Array(Array::new().items(RefOr::Type(
                Schema::object(Object::new().property("element", RefOr::Ref(Ref::new("#/test")))),
            ))))
            .item(Schema::Array(Array::new().items(RefOr::Type(
                Schema::object(Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar")))),
            ))));

        let serialized_json = serde_json::to_string(&a).expect("should serialize to json");
        let b: OneOf = serde_json::from_str(&serialized_json).expect("should deserialize OneOf");
        let reserialized_json = serde_json::to_string(&b).expect("reserialized json");

        assert_eq!(serialized_json, reserialized_json);
    }

    #[test]
    fn serialize_deserialize_object_with_multiple_schema_types() {
        let object =
            Object::new().schema_type(SchemaType::from_iter([BasicType::Object, BasicType::Null]));

        let json_str = serde_json::to_string(&object).expect("serde json should success");
        let deserialized: Object =
            serde_json::from_str(&json_str).expect("serde json should success");
        let json_de_str = serde_json::to_string(&deserialized).expect("serde json should success");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn test_empty_schema() {
        let schema = empty();
        assert_json_eq!(
            schema,
            json!({
                "default": null
            })
        )
    }

    #[test]
    fn test_default_schema() {
        let schema = Schema::default();
        assert_json_eq!(
            schema,
            json!({
                "type": "object",
            })
        )
    }

    #[test]
    fn test_ref_from_response_name() {
        let _ref = Ref::from_response_name("MyResponse");
        assert_json_eq!(
            _ref,
            json!({
                "$ref": "#/components/responses/MyResponse"
            })
        )
    }

    #[test]
    fn test_additional_properties_from_ref_or() {
        let additional_properties =
            AdditionalProperties::from(RefOr::Type(Schema::Object(Box::default())));
        assert_json_eq!(
            additional_properties,
            json!({
                "type": "object",
            })
        )
    }
}
