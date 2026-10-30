//! Parameter lists retain references while preserving inline construction and merge behavior.

use salvo_oapi::{OpenApi, Operation, Parameter, ParameterIn, Parameters, PathItem, Ref, RefOr};
use serde_json::{Value, json};

fn referenced_document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Shared parameters", "version": "1" },
        "paths": {
            "/pets/{id}": {
                "parameters": [{ "$ref": "#/components/parameters/PetId", "description": "Path override" }],
                "get": {
                    "parameters": [
                        { "$ref": "#/components/parameters/Limit" },
                        { "$ref": "./shared.yaml#/components/parameters/Trace", "summary": "External reference" },
                        { "name": "filter", "in": "query", "schema": { "type": "string" } }
                    ],
                    "responses": { "200": { "description": "OK" } }
                }
            }
        },
        "components": {
            "parameters": {
                "PetId": { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
                "Limit": { "name": "limit", "in": "query", "schema": { "type": "integer" } }
            }
        }
    })
}

fn assert_parameter_round_trip(input: Value, document: OpenApi) {
    let output = serde_json::to_value(&document).unwrap();
    let path = "/pets/{id}";
    assert_eq!(
        output["paths"][path]["parameters"],
        input["paths"][path]["parameters"]
    );
    assert_eq!(
        output["paths"][path]["get"]["parameters"],
        input["paths"][path]["get"]["parameters"]
    );
    assert_eq!(
        output["components"]["parameters"],
        input["components"]["parameters"]
    );
}

#[test]
fn path_and_operation_parameter_references_round_trip_json() {
    let input = referenced_document();
    let document: OpenApi =
        serde_json::from_value(input.clone()).expect("valid parameter references");
    assert_parameter_round_trip(input, document);
}

#[cfg(feature = "yaml")]
#[test]
fn path_and_operation_parameter_references_round_trip_yaml() {
    let input = referenced_document();
    let yaml = serde_norway::to_string(&input).unwrap();
    let document: OpenApi = serde_norway::from_str(&yaml).expect("valid YAML parameter references");
    let yaml = serde_norway::to_string(&document).expect("serialize YAML parameter references");
    assert_parameter_round_trip(input, serde_norway::from_str(&yaml).unwrap());
}

#[test]
fn builders_accept_inline_parameters_and_references() {
    let reference = Ref::new("#/components/parameters/Limit");
    let inline = Parameter::new("id").location(ParameterIn::Path);
    let operation = Operation::new()
        .parameters([reference.clone()])
        .add_parameter(inline.clone());
    let path = PathItem::default().parameters(operation.parameters.clone());
    assert_eq!(path.parameters, operation.parameters);
    let value = serde_json::to_value(operation).unwrap();
    assert_eq!(
        value["parameters"][0],
        json!({ "$ref": reference.ref_location })
    );
    assert_eq!(value["parameters"][1]["required"], true);

    let mut parameters = Parameters::from(vec![inline]);
    parameters.insert(reference.clone());
    assert!(parameters.contains("id", ParameterIn::Path));
    assert!(!parameters.contains("limit", ParameterIn::Query));
    assert_eq!(parameters.iter_inline().count(), 1);
    parameters.iter_inline_mut().next().unwrap().description = Some("Inline override".into());
    assert!(matches!(parameters.0[1], RefOr::Ref(_)));
    assert_eq!(
        parameters
            .iter_inline()
            .next()
            .unwrap()
            .description
            .as_deref(),
        Some("Inline override")
    );
}

#[test]
fn append_merges_inline_identity_and_replaces_matching_reference_metadata() {
    let reference = Ref::new("#/components/parameters/Limit").description("Old override");
    let mut parameters = Parameters::new()
        .parameter(reference)
        .parameter(Parameter::new("limit").location(ParameterIn::Query));
    let mut other = Parameters::new()
        .parameter(Ref::new("#/components/parameters/Limit").summary("New override"))
        .parameter(
            Parameter::new("limit")
                .location(ParameterIn::Query)
                .description("Merged"),
        )
        .parameter(Parameter::new("limit").location(ParameterIn::Header))
        .parameter(Ref::new("./other.yaml#/components/parameters/Limit"));
    parameters.append(&mut other);
    assert!(other.is_empty());
    assert_eq!(parameters.0.len(), 4);
    let value = serde_json::to_value(&parameters).unwrap();
    assert_eq!(
        value[0],
        json!({ "$ref": "#/components/parameters/Limit", "summary": "New override" })
    );
    assert_eq!(value[1]["description"], "Merged");
    assert_eq!(value[2]["in"], "header");
    assert_eq!(
        value[3]["$ref"],
        "./other.yaml#/components/parameters/Limit"
    );
    assert_eq!(parameters.into_iter().count(), 4);
}
