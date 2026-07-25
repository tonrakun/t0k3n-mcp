//! Trim schemars boilerplate out of the JSON Schemas advertised in `tools/list`.
//!
//! Every tool schema is carried by the client on every request, so anything the
//! client never reads is a permanent tax on the context window. Measured on the
//! default roster (79 tools), the `tools/list` payload is ~52.7k characters, of
//! which `$schema` (79 occurrences) and the `title: "XxxParams"` schemars derives
//! from the Rust struct name (78 occurrences) account for ~6.3k characters — none
//! of it consumed by an MCP client, which already knows these objects are
//! draft-07 tool-argument schemas.
//!
//! This is a presentation-only trim: validation semantics are untouched, so no
//! call that succeeded before can fail after.

use rmcp::model::JsonObject;
use serde_json::{Map, Value};

/// Schema *keywords* removed wherever a subschema appears.
///
/// - `$schema` is redundant once the object is nested inside an MCP `inputSchema`.
/// - `title` is a schemars artifact of the Rust type name (`ReadGitLogParams`)
///   and never describes anything the caller needs.
/// - `nullable` is OpenAPI 3.0 vocabulary, not a draft-07 keyword, so validators
///   already ignore it — and every field carrying it is an `Option<T>` that is
///   absent from `required`, which is how draft-07 says "optional" anyway. It
///   costs 1.5k characters to restate what the schema already conveys.
const DROPPED_KEYS: &[&str] = &["$schema", "title", "nullable"];

/// Keywords whose value is a map from *names* to subschemas. Their keys are
/// caller-facing identifiers, not keywords: `properties.title` is the `title`
/// argument of `task_create`, and deleting it would change the tool's contract.
const NAME_KEYED_MAPS: &[&str] = &["properties", "definitions", "$defs", "patternProperties"];

/// Keywords whose value is an *instance* (example data), not a subschema. A
/// default value may legitimately contain a `title` field of its own, so the
/// trim must not descend into these.
const INSTANCE_VALUES: &[&str] = &["default", "const", "enum", "examples"];

/// Strip the boilerplate keywords from a tool input schema, recursing through
/// every nested subschema.
pub(crate) fn slim_schema(schema: &mut JsonObject) {
    slim_subschema(schema);
}

/// `map` is itself a subschema: its keys are JSON Schema keywords.
fn slim_subschema(map: &mut Map<String, Value>) {
    for key in DROPPED_KEYS {
        map.remove(*key);
    }
    for (key, value) in map.iter_mut() {
        if INSTANCE_VALUES.contains(&key.as_str()) {
            continue;
        }
        if NAME_KEYED_MAPS.contains(&key.as_str()) {
            // One level down the keys are names; the values below them are schemas.
            if let Value::Object(named) = value {
                for schema in named.values_mut() {
                    slim_value(schema);
                }
            }
            continue;
        }
        slim_value(value);
    }
}

/// A schema fragment reached from a keyword: either a subschema object, or an
/// array of them (`anyOf`, `allOf`, tuple `items`). Traversing both shapes
/// generically means a future schemars layout cannot silently escape the trim.
fn slim_value(value: &mut Value) {
    match value {
        Value::Object(map) => slim_subschema(map),
        Value::Array(items) => {
            for item in items {
                slim_value(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(value: Value) -> JsonObject {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn drops_root_boilerplate_but_keeps_the_contract() {
        let mut schema = object(json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "ReadGitLogParams",
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "description": "Root-relative file path", "type": "string" }
            }
        }));

        slim_schema(&mut schema);

        assert!(!schema.contains_key("$schema"));
        assert!(!schema.contains_key("title"));
        // Everything that governs whether a call validates must survive.
        assert_eq!(schema["type"], json!("object"));
        assert_eq!(schema["required"], json!(["path"]));
        assert_eq!(
            schema["properties"]["path"]["description"],
            json!("Root-relative file path"),
            "field descriptions are what the agent actually reads"
        );
    }

    #[test]
    fn recurses_into_nested_subschemas() {
        // Mirrors batch_read: a definition with its own title, plus a property
        // whose type is an array of those definitions.
        let mut schema = object(json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "BatchReadParams",
            "definitions": {
                "BatchReadItem": {
                    "title": "BatchReadItem",
                    "type": "object",
                    "properties": {
                        "id": { "title": "Id", "type": "string" }
                    }
                }
            },
            "properties": {
                "reads": {
                    "type": "array",
                    "items": { "$ref": "#/definitions/BatchReadItem" }
                },
                "mode": {
                    "anyOf": [
                        { "title": "Fast", "type": "string" },
                        { "title": "Null", "type": "null" }
                    ]
                }
            }
        }));

        slim_schema(&mut schema);

        let rendered = serde_json::to_string(&schema).unwrap();
        assert!(
            !rendered.contains("\"title\""),
            "nested titles must be trimmed too: {rendered}"
        );
        assert!(
            !rendered.contains("$schema"),
            "nested $schema must be trimmed too: {rendered}"
        );
        // The definition itself must survive — only its title goes.
        assert_eq!(
            schema["definitions"]["BatchReadItem"]["properties"]["id"]["type"],
            json!("string")
        );
        // The $ref target must still resolve after the trim.
        assert_eq!(
            schema["properties"]["reads"]["items"]["$ref"],
            json!("#/definitions/BatchReadItem")
        );
        assert_eq!(
            schema["properties"]["mode"]["anyOf"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn drops_nullable_without_touching_optionality() {
        // `required` is what actually makes a field optional in draft-07; the
        // OpenAPI-flavoured `nullable` alongside it is pure restatement.
        let mut schema = object(json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" },
                "depth": { "type": "integer", "nullable": true, "minimum": 0 }
            }
        }));

        slim_schema(&mut schema);

        assert!(schema["properties"]["depth"].get("nullable").is_none());
        assert_eq!(
            schema["properties"]["depth"]["minimum"],
            json!(0),
            "real validation keywords must survive"
        );
        assert_eq!(
            schema["required"],
            json!(["path"]),
            "optionality is expressed by `required`, which must not change"
        );
    }

    #[test]
    fn keeps_a_property_named_nullable() {
        let mut schema = object(json!({
            "type": "object",
            "properties": { "nullable": { "type": "boolean" } }
        }));

        slim_schema(&mut schema);

        assert!(
            schema["properties"].get("nullable").is_some(),
            "a key under `properties` is an argument name, not a keyword"
        );
    }

    #[test]
    fn is_idempotent_and_safe_on_an_already_slim_schema() {
        let mut schema = object(json!({ "type": "object", "properties": {} }));
        let before = schema.clone();
        slim_schema(&mut schema);
        slim_schema(&mut schema);
        assert_eq!(schema, before);
    }

    #[test]
    fn does_not_drop_a_property_that_is_merely_named_title() {
        // task_create really does take a `title` argument. Only schema *keywords*
        // are boilerplate; a key under `properties` is part of the contract.
        let mut schema = object(json!({
            "title": "TaskCreateParams",
            "type": "object",
            "required": ["title"],
            "properties": {
                "title": { "description": "Task title", "type": "string" },
                "status": { "description": "Status", "type": "string" }
            }
        }));

        slim_schema(&mut schema);

        assert!(
            !schema.contains_key("title"),
            "the root title is boilerplate"
        );
        assert_eq!(
            schema["properties"]["title"]["description"],
            json!("Task title"),
            "a property named `title` is an argument, not schemars boilerplate"
        );
        assert_eq!(schema["required"], json!(["title"]));
    }

    #[test]
    fn does_not_descend_into_default_values() {
        // A default that happens to contain `title` is data, not a subschema.
        let mut schema = object(json!({
            "title": "Params",
            "type": "object",
            "properties": {
                "layout": {
                    "type": "object",
                    "default": { "title": "untitled", "columns": 2 }
                }
            }
        }));

        slim_schema(&mut schema);

        assert_eq!(
            schema["properties"]["layout"]["default"],
            json!({ "title": "untitled", "columns": 2 }),
            "default values must be preserved verbatim"
        );
    }
}
