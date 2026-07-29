//! `inspect capabilities --emit schema`, checked against the catalog it claims
//! to describe.
//!
//! A published schema that does not validate the real document is worse than no
//! schema: a consumer generates types from it and then fails on the first live
//! response. So this file does not assert the schema's *text*; it runs the
//! catalog through it.
//!
//! The validator below is deliberately small. It implements only the keywords
//! this schema uses — `type`, `const`, `enum`, `required`, `properties`,
//! `additionalProperties`, `items`, `minimum`, and `$ref` into `$defs` — and
//! *fails* on any keyword it does not know, so a keyword added to the schema
//! cannot silently stop being checked. Pulling in a full JSON Schema validator
//! would add a dependency to a workspace that curates its dependency list, for
//! a job that is a hundred lines at this subset.

use super::*;
use serde_json::Value;

fn emit(args: &[&str]) -> Value {
    let assert = paredit().args(args).assert().success();
    serde_json::from_slice(&assert.get_output().stdout).expect("output parses as json")
}

fn catalog(version: &str) -> Value {
    emit(&[
        "inspect",
        "capabilities",
        "--output",
        "json",
        "--schema-version",
        version,
    ])
}

fn schema(version: &str) -> Value {
    emit(&[
        "inspect",
        "capabilities",
        "--emit",
        "schema",
        "--schema-version",
        version,
    ])
}

/// Validates `instance` against `schema`, resolving `$ref` in `root`.
///
/// Returns every failure rather than the first: one missing property in a
/// deeply nested command is easier to fix alongside its siblings than one
/// re-run at a time.
fn validate(instance: &Value, schema: &Value, root: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        errors.push(format!("{path}: schema is not an object"));
        return;
    };

    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        let name = reference
            .strip_prefix("#/$defs/")
            .unwrap_or_else(|| panic!("{path}: only #/$defs/ refs are emitted, got {reference}"));
        let target = &root["$defs"][name];
        assert!(!target.is_null(), "{path}: dangling $ref {reference}");
        validate(instance, target, root, path, errors);
        return;
    }

    for keyword in object.keys() {
        assert!(
            matches!(
                keyword.as_str(),
                "$schema"
                    | "$id"
                    | "$defs"
                    | "title"
                    | "description"
                    | "type"
                    | "const"
                    | "enum"
                    | "required"
                    | "properties"
                    | "additionalProperties"
                    | "items"
                    | "minimum"
            ),
            "{path}: unhandled schema keyword {keyword:?}; teach the validator about it \
             rather than letting it go unchecked"
        );
    }

    if let Some(expected) = object.get("const") {
        if instance != expected {
            errors.push(format!(
                "{path}: expected const {expected}, found {instance}"
            ));
        }
    }

    if let Some(allowed) = object.get("enum").and_then(Value::as_array) {
        if !allowed.contains(instance) {
            errors.push(format!("{path}: {instance} is not one of {allowed:?}"));
        }
    }

    if let Some(types) = object.get("type") {
        let names: Vec<&str> = match types {
            Value::String(name) => vec![name.as_str()],
            Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
            other => panic!("{path}: unexpected `type` {other}"),
        };
        let matched = names.iter().any(|name| match *name {
            "object" => instance.is_object(),
            "array" => instance.is_array(),
            "string" => instance.is_string(),
            "integer" => instance.is_i64() || instance.is_u64(),
            "boolean" => instance.is_boolean(),
            "null" => instance.is_null(),
            other => panic!("{path}: unhandled type name {other}"),
        });
        if !matched {
            errors.push(format!("{path}: {instance} is not of type {names:?}"));
        }
    }

    if let Some(minimum) = object.get("minimum").and_then(Value::as_i64) {
        if instance.as_i64().is_some_and(|value| value < minimum) {
            errors.push(format!("{path}: {instance} is below minimum {minimum}"));
        }
    }

    if let Some(members) = instance.as_object() {
        for required in object
            .get("required")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
        {
            let name = required.as_str().unwrap_or_default();
            if !members.contains_key(name) {
                errors.push(format!("{path}: missing required property {name:?}"));
            }
        }

        let properties = object.get("properties").and_then(Value::as_object);
        for (name, value) in members {
            match properties.and_then(|properties| properties.get(name)) {
                Some(property) => {
                    validate(value, property, root, &format!("{path}.{name}"), errors);
                }
                None => match object.get("additionalProperties") {
                    Some(Value::Bool(false)) => errors.push(format!(
                        "{path}: property {name:?} is not described and additionalProperties \
                         is false"
                    )),
                    Some(Value::Bool(true)) | None => {}
                    Some(additional) => {
                        validate(value, additional, root, &format!("{path}.{name}"), errors);
                    }
                },
            }
        }
    }

    if let (Some(values), Some(items)) = (instance.as_array(), object.get("items")) {
        for (index, value) in values.iter().enumerate() {
            validate(value, items, root, &format!("{path}[{index}]"), errors);
        }
    }
}

fn assert_validates(version: &str) {
    let document = catalog(version);
    let schema = schema(version);
    let mut errors = Vec::new();
    validate(&document, &schema, &schema, "$", &mut errors);
    assert!(
        errors.is_empty(),
        "the published schema does not validate its own catalog at version {version}:\n{}",
        errors.join("\n")
    );
}

#[test]
fn cli_capabilities_schema_validates_the_catalog_it_describes() {
    for version in ["1", "2", "3"] {
        assert_validates(version);
    }
}

#[test]
fn cli_capabilities_schema_is_draft_2020_12_and_names_its_version() {
    let schema = schema("2");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["properties"]["schema_version"]["const"], 2);
    assert!(
        schema["$id"]
            .as_str()
            .is_some_and(|id| id.ends_with("capabilities-v2.json")),
        "each version publishes under its own $id: {schema}"
    );
}

/// The version number on the catalog is only worth carrying if the schemas
/// differ, and the difference has to be in the direction that catches drift: a
/// version 1 schema must reject a version 3 document.
#[test]
fn cli_capabilities_schema_for_version_one_rejects_a_version_three_catalog() {
    let document = catalog("3");
    let schema = schema("1");
    let mut errors = Vec::new();
    validate(&document, &schema, &schema, "$", &mut errors);
    assert!(
        !errors.is_empty(),
        "a version 1 schema must not validate a version 3 catalog"
    );
}

/// The schema has to describe the dialect contract's `silent` status from
/// version 3, because that is where the contract stops folding it onto
/// `unsupported` — and a consumer validating a v3 catalog against a schema that
/// omits the word would reject a correct document.
#[test]
fn cli_capabilities_schema_admits_the_silent_status_only_from_version_three() {
    let statuses = |version: &str| {
        schema(version)["$defs"]["dialectContract"]["properties"]["statuses"]["items"]["enum"]
            .clone()
    };
    assert_eq!(
        statuses("2"),
        serde_json::json!(["supported", "unsupported", "unknown"])
    );
    assert_eq!(
        statuses("3"),
        serde_json::json!(["supported", "silent", "unsupported", "unknown"])
    );
}

#[test]
fn cli_capabilities_defaults_to_the_catalog() {
    let default = emit(&["inspect", "capabilities", "--output", "json"]);
    assert!(default["commands"].is_array(), "{default}");
    assert!(default["$schema"].is_null(), "{default}");
}
