use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde_json::{json, Map, Value};

use crate::error::CliError;

const RAW_SCHEMA: &str = include_str!("../../../schemas/api-v1.schema.json");

static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(RAW_SCHEMA)
        .expect("embedded schemas/api-v1.schema.json must be valid JSON")
});

/// Return a reference to the cached parsed `api-v1.schema.json` document.
pub fn raw_schema() -> &'static Value {
    &SCHEMA
}

/// Look up the `x-methods` specification object for `method`.
pub fn method_spec(method: &str) -> Option<&'static Value> {
    raw_schema().get("x-methods")?.get(method)
}

/// Return the scope of `method` (`"collection"` or `"application"`), if known.
pub fn method_scope(method: &str) -> Option<&'static str> {
    method_spec(method)?.get("scope")?.as_str()
}

/// Return the completion mode of `method` (`"completed"` or `"operation"`), if known.
pub fn method_completion(method: &str) -> Option<&'static str> {
    method_spec(method)?.get("completion")?.as_str()
}

/// Return all method names under `x-methods` in deterministic sorted order.
pub fn all_method_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = raw_schema()
        .get("x-methods")
        .and_then(Value::as_object)
        .map(|methods| methods.keys().map(String::as_str).collect())
        .unwrap_or_default();
    names.sort_unstable();
    names
}

/// List all API v1 methods from the embedded schema, optionally filtered by substring.
pub fn list_methods(filter: Option<&str>) -> Result<Value, CliError> {
    let schema = raw_schema();
    let methods = schema
        .get("x-methods")
        .and_then(Value::as_object)
        .ok_or_else(|| CliError::SchemaNotFound("Missing x-methods in embedded schema".into()))?;

    let needle = filter.map(|s| s.to_ascii_lowercase());
    let mut items = Vec::new();

    for (name, spec) in methods {
        if let Some(ref n) = needle {
            if !name.to_ascii_lowercase().contains(n) {
                continue;
            }
        }
        items.push(json!({
            "method": name,
            "summary": spec.get("summary").cloned().unwrap_or(Value::Null),
            "scope": spec.get("scope").cloned().unwrap_or(Value::Null),
            "effect": spec.get("effect").cloned().unwrap_or(Value::Null),
            "completion": spec.get("completion").cloned().unwrap_or(Value::Null),
            "undo": spec.get("undo").cloned().unwrap_or(Value::Null),
        }));
    }

    Ok(json!({
        "count": items.len(),
        "methods": items,
    }))
}

/// Describe a single API v1 method, including its params/result schemas and all
/// transitively referenced `$defs` so an LLM agent can construct valid requests.
pub fn describe_method(method: &str) -> Result<Value, CliError> {
    let schema = raw_schema();
    let methods = schema
        .get("x-methods")
        .and_then(Value::as_object)
        .ok_or_else(|| CliError::SchemaNotFound("Missing x-methods in embedded schema".into()))?;

    let spec = methods
        .get(method)
        .ok_or_else(|| CliError::SchemaNotFound(format!("Unknown method '{method}'")))?;

    let defs = schema
        .get("$defs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut needed = BTreeSet::new();
    if let Some(p) = spec.get("params") {
        collect_refs(p, &defs, &mut needed);
    }
    if let Some(r) = spec.get("result") {
        collect_refs(r, &defs, &mut needed);
    }

    let mut resolved_defs = BTreeMap::new();
    for key in needed {
        if let Some(def) = defs.get(&key) {
            resolved_defs.insert(key, def.clone());
        }
    }

    Ok(json!({
        "method": method,
        "summary": spec.get("summary").cloned().unwrap_or(Value::Null),
        "scope": spec.get("scope").cloned().unwrap_or(Value::Null),
        "effect": spec.get("effect").cloned().unwrap_or(Value::Null),
        "completion": spec.get("completion").cloned().unwrap_or(Value::Null),
        "undo": spec.get("undo").cloned().unwrap_or(Value::Null),
        "errors": spec.get("errors").cloned().unwrap_or(Value::Null),
        "notes": spec.get("notes").cloned().unwrap_or(Value::Null),
        "params": spec.get("params").cloned().unwrap_or(Value::Null),
        "result": spec.get("result").cloned().unwrap_or(Value::Null),
        "$defs": resolved_defs,
    }))
}

/// Return documented API error types and limits from the embedded schema.
pub fn list_errors_and_limits() -> Result<Value, CliError> {
    let schema = raw_schema();
    Ok(json!({
        "errors": schema.get("x-errors").cloned().unwrap_or(Value::Null),
        "limits": schema.get("x-limits").cloned().unwrap_or(Value::Null),
    }))
}

/// Validate `params` for `method` against its embedded JSON Schema in `api-v1.schema.json`.
pub fn validate_method_params(method: &str, params: &Value) -> Result<(), CliError> {
    let spec = method_spec(method)
        .ok_or_else(|| CliError::SchemaNotFound(format!("Unknown method '{method}'")))?;

    let Some(params_schema) = spec.get("params") else {
        return Ok(());
    };

    let defs = raw_schema()
        .get("$defs")
        .and_then(Value::as_object)
        .ok_or_else(|| CliError::SchemaNotFound("Missing $defs in embedded schema".into()))?;

    validate_value_against_schema(params, params_schema, defs, "$").map_err(|err| match err {
        CliError::InvalidInput(detail) => {
            CliError::InvalidInput(format!("Schema validation failed for '{method}' {detail}"))
        }
        other => other,
    })
}

fn validate_value_against_schema(
    value: &Value,
    schema_node: &Value,
    defs: &Map<String, Value>,
    path: &str,
) -> Result<(), CliError> {
    if let Some(b) = schema_node.as_bool() {
        return if b {
            Ok(())
        } else {
            Err(CliError::InvalidInput(format!(
                "({path}): value is not allowed by schema"
            )))
        };
    }

    let Some(obj) = schema_node.as_object() else {
        return Ok(());
    };

    if let Some(Value::String(reference)) = obj.get("$ref") {
        if let Some(name) = reference.strip_prefix("#/$defs/") {
            let target = defs.get(name).ok_or_else(|| {
                CliError::SchemaNotFound(format!("Unknown schema definition '{name}'"))
            })?;
            validate_value_against_schema(value, target, defs, path)?;
        }
    }

    if let Some(const_val) = obj.get("const") {
        if value != const_val {
            return Err(CliError::InvalidInput(format!(
                "({path}): expected constant value {const_val}, got {value}"
            )));
        }
    }

    if let Some(Value::Array(enum_arr)) = obj.get("enum") {
        if !enum_arr.contains(value) {
            return Err(CliError::InvalidInput(format!(
                "({path}): value {value} is not one of the allowed enum values"
            )));
        }
    }

    if let Some(Value::Array(branches)) = obj.get("allOf") {
        for branch in branches {
            validate_value_against_schema(value, branch, defs, path)?;
        }
    }

    if let Some(Value::Array(branches)) = obj.get("anyOf") {
        let matched = branches
            .iter()
            .any(|branch| validate_value_against_schema(value, branch, defs, path).is_ok());
        if !matched {
            return Err(CliError::InvalidInput(format!(
                "({path}): value does not match any of the allowed schemas in anyOf"
            )));
        }
    }

    if let Some(Value::Array(branches)) = obj.get("oneOf") {
        let match_count = branches
            .iter()
            .filter(|branch| validate_value_against_schema(value, branch, defs, path).is_ok())
            .count();
        if match_count != 1 {
            return Err(CliError::InvalidInput(format!(
                "({path}): value must match exactly one schema in oneOf (matched {match_count})"
            )));
        }
    }

    if let Some(not_schema) = obj.get("not") {
        if validate_value_against_schema(value, not_schema, defs, path).is_ok() {
            return Err(CliError::InvalidInput(format!(
                "({path}): value must not match disallowed schema"
            )));
        }
    }

    if let Some(if_schema) = obj.get("if") {
        if validate_value_against_schema(value, if_schema, defs, path).is_ok() {
            if let Some(then_schema) = obj.get("then") {
                validate_value_against_schema(value, then_schema, defs, path)?;
            }
        } else if let Some(else_schema) = obj.get("else") {
            validate_value_against_schema(value, else_schema, defs, path)?;
        }
    }

    if let Some(type_val) = obj.get("type") {
        check_json_type(value, type_val, path)?;
    }

    match value {
        Value::Object(map) => validate_object(map, obj, defs, path)?,
        Value::Array(arr) => validate_array(arr, obj, defs, path)?,
        Value::String(s) => validate_string(s, obj, path)?,
        Value::Number(n) => validate_number(n, obj, path)?,
        _ => {}
    }

    Ok(())
}

fn check_json_type(value: &Value, type_val: &Value, path: &str) -> Result<(), CliError> {
    match type_val {
        Value::String(expected) => {
            if !matches_type(value, expected) {
                return Err(CliError::InvalidInput(format!(
                    "({path}): expected type '{expected}', got {}",
                    json_type_name(value)
                )));
            }
        }
        Value::Array(allowed) => {
            let matched = allowed
                .iter()
                .filter_map(Value::as_str)
                .any(|expected| matches_type(value, expected));
            if !matched {
                return Err(CliError::InvalidInput(format!(
                    "({path}): value type '{}' is not in allowed types",
                    json_type_name(value)
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

fn matches_type(value: &Value, expected: &str) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn validate_object(
    map: &Map<String, Value>,
    obj: &Map<String, Value>,
    defs: &Map<String, Value>,
    path: &str,
) -> Result<(), CliError> {
    if let Some(Value::Array(required)) = obj.get("required") {
        for req in required {
            if let Some(key) = req.as_str() {
                if !map.contains_key(key) {
                    return Err(CliError::InvalidInput(format!(
                        "({path}): missing required property '{key}'"
                    )));
                }
            }
        }
    }

    if let Some(min_props) = obj.get("minProperties").and_then(Value::as_u64) {
        if (map.len() as u64) < min_props {
            return Err(CliError::InvalidInput(format!(
                "({path}): object must have at least {min_props} properties"
            )));
        }
    }

    let props = obj.get("properties").and_then(Value::as_object);
    let additional = obj.get("additionalProperties");

    for (key, val) in map {
        let child_path = format!("{path}.{key}");
        if let Some(prop_schema) = props.and_then(|p| p.get(key)) {
            validate_value_against_schema(val, prop_schema, defs, &child_path)?;
        } else if let Some(add_schema) = additional {
            if add_schema == &Value::Bool(false) {
                return Err(CliError::InvalidInput(format!(
                    "({path}): unknown property '{key}' is not allowed"
                )));
            } else if add_schema.is_object() {
                validate_value_against_schema(val, add_schema, defs, &child_path)?;
            }
        }
    }

    Ok(())
}

fn validate_array(
    arr: &[Value],
    obj: &Map<String, Value>,
    defs: &Map<String, Value>,
    path: &str,
) -> Result<(), CliError> {
    if let Some(min_items) = obj.get("minItems").and_then(Value::as_u64) {
        if (arr.len() as u64) < min_items {
            return Err(CliError::InvalidInput(format!(
                "({path}): array must have at least {min_items} items"
            )));
        }
    }

    if let Some(max_items) = obj.get("maxItems").and_then(Value::as_u64) {
        if (arr.len() as u64) > max_items {
            return Err(CliError::InvalidInput(format!(
                "({path}): array must have at most {max_items} items"
            )));
        }
    }

    if obj.get("uniqueItems") == Some(&Value::Bool(true)) {
        for (i, item) in arr.iter().enumerate() {
            if arr[i + 1..].contains(item) {
                return Err(CliError::InvalidInput(format!(
                    "({path}): array items must be unique"
                )));
            }
        }
    }

    if let Some(items_schema) = obj.get("items") {
        if items_schema.is_object() || items_schema.is_boolean() {
            for (idx, item) in arr.iter().enumerate() {
                let item_path = format!("{path}[{idx}]");
                validate_value_against_schema(item, items_schema, defs, &item_path)?;
            }
        }
    }

    Ok(())
}

fn validate_string(s: &str, obj: &Map<String, Value>, path: &str) -> Result<(), CliError> {
    if let Some(min_len) = obj.get("minLength").and_then(Value::as_u64) {
        if (s.chars().count() as u64) < min_len {
            return Err(CliError::InvalidInput(format!(
                "({path}): string length must be at least {min_len}"
            )));
        }
    }

    if let Some(max_len) = obj.get("maxLength").and_then(Value::as_u64) {
        if (s.chars().count() as u64) > max_len {
            return Err(CliError::InvalidInput(format!(
                "({path}): string length must be at most {max_len}"
            )));
        }
    }

    if let Some(Value::String(pattern)) = obj.get("pattern") {
        if !matches_known_pattern(s, pattern) {
            return Err(CliError::InvalidInput(format!(
                "({path}): string '{s}' does not match pattern '{pattern}'"
            )));
        }
    }

    Ok(())
}

fn matches_known_pattern(s: &str, pattern: &str) -> bool {
    match pattern {
        "^[1-9][0-9]{0,18}$" => {
            !s.is_empty()
                && !s.starts_with('0')
                && s.len() <= 19
                && s.bytes().all(|b| b.is_ascii_digit())
                && s.parse::<i64>().is_ok_and(|n| n > 0)
        }
        "[^0-9]" => s.bytes().any(|b| !b.is_ascii_digit()),
        "^$|^[0-9]{4}-[0-9]{2}-[0-9]{2}$" => {
            if s.is_empty() {
                return true;
            }
            let b = s.as_bytes();
            b.len() == 10
                && b[4] == b'-'
                && b[7] == b'-'
                && b[0..4].iter().all(u8::is_ascii_digit)
                && b[5..7].iter().all(u8::is_ascii_digit)
                && b[8..10].iter().all(u8::is_ascii_digit)
        }
        "^[0-9a-f]{64}$" => {
            s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        }
        "^[^/\\\\\\u0000]+$" => !s.is_empty() && !s.chars().any(|c| matches!(c, '/' | '\\' | '\0')),
        "^https?://" => s.starts_with("http://") || s.starts_with("https://"),
        _ => true,
    }
}

fn validate_number(
    n: &serde_json::Number,
    obj: &Map<String, Value>,
    path: &str,
) -> Result<(), CliError> {
    let Some(val) = n.as_f64() else {
        return Ok(());
    };

    if let Some(min) = obj.get("minimum").and_then(Value::as_f64) {
        if val < min {
            return Err(CliError::InvalidInput(format!(
                "({path}): value {n} must be >= {min}"
            )));
        }
    }

    if let Some(max) = obj.get("maximum").and_then(Value::as_f64) {
        if val > max {
            return Err(CliError::InvalidInput(format!(
                "({path}): value {n} must be <= {max}"
            )));
        }
    }

    if let Some(ex_min) = obj.get("exclusiveMinimum").and_then(Value::as_f64) {
        if val <= ex_min {
            return Err(CliError::InvalidInput(format!(
                "({path}): value {n} must be > {ex_min}"
            )));
        }
    }

    if let Some(ex_max) = obj.get("exclusiveMaximum").and_then(Value::as_f64) {
        if val >= ex_max {
            return Err(CliError::InvalidInput(format!(
                "({path}): value {n} must be < {ex_max}"
            )));
        }
    }

    Ok(())
}

fn collect_refs(
    value: &Value,
    all_defs: &serde_json::Map<String, Value>,
    visited: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                if let Some(name) = reference.strip_prefix("#/$defs/") {
                    if visited.insert(name.to_string()) {
                        if let Some(target) = all_defs.get(name) {
                            collect_refs(target, all_defs, visited);
                        }
                    }
                }
            }
            for v in map.values() {
                collect_refs(v, all_defs, visited);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_refs(v, all_defs, visited);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_all_methods() {
        let res = list_methods(None).expect("schema should parse");
        let count = res.get("count").and_then(Value::as_u64).unwrap_or(0);
        assert!(count >= 90, "expected >= 90 methods, got {count}");
    }

    #[test]
    fn describes_notes_create_with_defs() {
        let res = describe_method("notes.create").expect("notes.create should exist");
        let defs = res.get("$defs").and_then(Value::as_object).unwrap();
        assert!(defs.contains_key("notes_create_Params"));
        assert!(defs.contains_key("Reference"));
        assert!(defs.contains_key("NoteCreated"));
    }

    #[test]
    fn all_method_names_returns_all_96_methods_sorted() {
        let names = all_method_names();
        assert_eq!(names.len(), 96);
        assert!(names.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn method_scope_returns_expected_scopes() {
        assert_eq!(method_scope("decks.list"), Some("collection"));
        assert_eq!(method_scope("notes.create"), Some("collection"));
        assert_eq!(method_scope("operations.get"), Some("application"));
        assert_eq!(method_scope("api.describe"), Some("application"));
        assert_eq!(method_scope("api.execute"), Some("application"));
        assert_eq!(method_scope("profiles.list"), Some("application"));
        assert_eq!(method_scope("unknown.method"), None);
    }

    #[test]
    fn method_completion_returns_expected_modes() {
        assert_eq!(method_completion("collection.sync"), Some("operation"));
        assert_eq!(method_completion("api.execute"), Some("operation"));
        assert_eq!(method_completion("profiles.switch"), Some("operation"));
        assert_eq!(method_completion("decks.list"), Some("completed"));
        assert_eq!(method_completion("operations.get"), Some("completed"));
    }

    #[test]
    fn validate_method_params_accepts_valid_inputs() {
        assert!(validate_method_params(
            "notes.create",
            &json!({
                "deck": { "name": "Default" },
                "noteType": { "name": "Basic" },
                "fields": { "Front": "Question", "Back": "Answer" }
            }),
        )
        .is_ok());

        assert!(validate_method_params(
            "decks.get",
            &json!({ "deck": { "name": "Default" } }),
        )
        .is_ok());

        assert!(validate_method_params(
            "decks.get",
            &json!({ "deck": { "id": "1" } }),
        )
        .is_ok());

        assert!(validate_method_params(
            "operations.get",
            &json!({ "operationId": "op-1" }),
        )
        .is_ok());

        assert!(validate_method_params("api.describe", &json!({})).is_ok());
    }

    #[test]
    fn validate_method_params_rejects_invalid_inputs() {
        let unknown = validate_method_params("nonexistent.method", &json!({}));
        assert!(matches!(unknown, Err(CliError::SchemaNotFound(_))));

        let missing_required = validate_method_params("notes.get", &json!({}));
        assert!(matches!(missing_required, Err(CliError::InvalidInput(_))));

        let extra_prop = validate_method_params(
            "operations.get",
            &json!({ "operationId": "op-1", "session": "s-1" }),
        );
        assert!(matches!(extra_prop, Err(CliError::InvalidInput(_))));

        let zero_id = validate_method_params("notes.get", &json!({ "id": "0" }));
        assert!(matches!(zero_id, Err(CliError::InvalidInput(_))));

        let alpha_id = validate_method_params("notes.get", &json!({ "id": "abc" }));
        assert!(matches!(alpha_id, Err(CliError::InvalidInput(_))));

        let both_ref = validate_method_params(
            "decks.get",
            &json!({ "deck": { "id": "1", "name": "Default" } }),
        );
        assert!(matches!(both_ref, Err(CliError::InvalidInput(_))));

        let empty_ref = validate_method_params("decks.get", &json!({ "deck": {} }));
        assert!(matches!(empty_ref, Err(CliError::InvalidInput(_))));
    }
}
